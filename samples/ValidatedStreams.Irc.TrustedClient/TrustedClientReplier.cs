using System.Collections.Concurrent;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using Google.Protobuf;
using IrcDotNet;

namespace ValidatedStreams.Irc;

/// How this works:
/// Conceptually, we want only one trusted client to respond to the user after their request has been handled.
/// Furthermore, we observe that the problem has the following constraints:
///  - We cannot ensure that another validator would do send a message on IRC.
///    (As a trusted client, and we cannot (currently) slash other validators based on their off-chain actions.)
///  - We want to minimize the chance two trusted clients reply for the same event.
///  - We want to minimize the time between receiving a validated event and telling the user about it.
///  - We don't know who is who except by their (untrusted) IRC user details (though that could be changed.)
/// We could do something like AURA, and hand off a slot based on the current unix time, but this still leaves
/// the risk that two validators with slightly misaligned internal clocks would end up sending their messages at
/// the same time.
/// So, we do something else, and order the list of validators by some predictable value; for example the hash
/// of the event and the validator's nickname. Then, we wait based on our place in the list -- the later we come,
/// the later we are going to send a reply. If anyone replies earlier, we cancel the timer -- since the reply is
/// already in.
/// In fact, we apply a nonlinear function to the order; that way, an attacker seeking to slow the replies down
/// by adding a bunch of "fake" trusted clients, would need a non-linear amount of fake clients for only any given
/// increase in wait time. Here we use sqrt(x) as being less likely to swamp the user in case of attack than log(x-1).
/// This has the drawback that unresponsive clients won't be removed from the list despite not sending messages,
/// but on the plus side, it is a really simple way of solving the problem at hand.
public class TrustedClientReplier
{
    private ConcurrentDictionary<ByteString, Timer?> ReplyTimers = new();
    private ByteArrayComparer ByteArrayComparer = new();
    private IrcChannel IrcChannel;
    private Regex UsernameRegex;
    private Regex ReplyRegex;
    private string ReplyFormat;
    private TrustedClientUserTracker UserTracker;

    public TrustedClientReplier(IrcChannel ircChannel, (Regex, string) replyFormat, (Regex, string) usernameFormat, TrustedClientUserTracker userTracker)
    {
        IrcChannel = ircChannel;
        (ReplyRegex, ReplyFormat) = replyFormat;
        UsernameRegex = usernameFormat.Item1;
        UserTracker = userTracker;

        ircChannel.MessageReceived += (sender, ev) =>
        {
            var replyMatch = ReplyRegex.Match(ev.Text);
            if (replyMatch.Success)
            {
                var hashHex = replyMatch.Groups["eventId"].Value;
                var hash = ByteString.CopyFrom(Convert.FromHexString(hashHex));
                Console.WriteLine("Reply for {0}, someone else sent it; cancelling!", hashHex);
                ReplyTimers.AddOrUpdate(hash, _ => null, (_, timer) => { timer?.Dispose(); return null; });
            }
        };
    }

    public void SendReply(ByteString hash)
    {
        using var sha256 = SHA256.Create();
        var orderedChannelClients =
            IrcChannel.Users
            .Select(channelUser => channelUser.User)
            .Where(user => UsernameRegex.Match(user.UserName ?? "").Success)
            .OrderBy(user =>
                sha256.ComputeHash(hash.ToByteArray().Concat(Encoding.UTF8.GetBytes(user.NickName)).ToArray()
            ), ByteArrayComparer);

        //Console.WriteLine("Order: {0}", string.Join(", ", orderedChannelClients.Select(x => $"{x.NickName}-{x.UserName}")));

        var ownNumber = orderedChannelClients.TakeWhile(x => !(x is IrcLocalUser)).Count();
        var timeToWait = SlotToTime(ownNumber);

        Console.WriteLine("Reply to {0} for {1}, time to wait {2} => {3}", ReplyFormat, Convert.ToHexString(hash.ToByteArray()), ownNumber, timeToWait);

        ReplyTimers.AddOrUpdate(hash,
            _ => new Timer(TimerCallback, hash, timeToWait, Timeout.InfiniteTimeSpan),
            (_, timer) => timer
        );
    }

    private void TimerCallback(object? hashO)
    {
        var hash = (ByteString)hashO!;
        var hashHex = Convert.ToHexString(hash.ToByteArray());

        Console.WriteLine("Reply for {0}, timer callback hit; replying!", hashHex);
        ReplyTimers.AddOrUpdate(hash, _ => null, (_, timer) => { timer?.Dispose(); return null; });

        var user = UserTracker.GetOriginUser(hash) ?? "<unknown user>";
        IrcChannel.Client.LocalUser.SendMessage(IrcChannel, String.Format(ReplyFormat, hashHex, user));
    }

    protected virtual TimeSpan SlotToTime(int slot)
    {
        return TimeSpan.FromSeconds(Math.Sqrt(slot * 1.0f) * 2.0f);
    }
}
