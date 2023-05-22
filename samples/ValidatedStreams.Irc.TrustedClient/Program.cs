using System.Collections.Concurrent;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using Google.Protobuf;
using Grpc.Core;
using Grpc.Net.Client;
using IrcDotNet;
using ValidatedStreams;

var grpc = Environment.GetCommandLineArgs()[1];
var server = Environment.GetCommandLineArgs()[2];
var channel = Environment.GetCommandLineArgs()[3];
var nickname = Environment.GetCommandLineArgs()[4];

var helpCommand = new Regex(@"^(?<nickname>[^!@]+).+ help.+$");
var witnessCommand = new Regex(@"^!w(?itness)? (?<data>.+)$");
var witnessCommandHelpText = "{0}: !w[itness] <data> -- create and witness a validated-streams event";
var witnessReply = new Regex(@"^(?<nickname>[^!@]+): (?<eventId>[A-Z0-9]+) witnessed!$");
var witnessReplyFormat = "{1}: {0} witnessed!";
var trustedClientRealName = new Regex(@"Validated Streams Trusted Client");

var realName = "Validated Streams Trusted Client";

var replyTimers = new ConcurrentDictionary<ByteString, Timer?>();
var eventUsers = new ConcurrentDictionary<ByteString, string>(); // Could also use IPFS for this

using var grpcChannel = GrpcChannel.ForAddress(grpc);
var streamsClient = new Streams.StreamsClient(grpcChannel);

using var sha256 = SHA256.Create();

var registrationInfo = new IrcUserRegistrationInfo()
{
    NickName = nickname,
    UserName = nickname,
    RealName = realName
};
var ircClient = new StandardIrcClient();
ircClient.FloodPreventer = new IrcStandardFloodPreventer(4, 2000);
// ircClient.Connected += IrcClient_Connected;
// ircClient.Disconnected += IrcClient_Disconnected;
// ircClient.Registered += IrcClient_Registered;

// Wait until connection has succeeded or timed out.
using (var connectedEvent = new ManualResetEventSlim(false))
{
    ircClient.Connected += (sender, e2) => connectedEvent.Set();
    ircClient.Connect(server, false, registrationInfo);
    if (!connectedEvent.Wait(10000))
    {
        ircClient.Dispose();
        Console.WriteLine("Connection to '{0}' timed out.", server);
        return;
    }
}


ircClient.Channels.Join(channel);

ircClient.LocalUser.MessageReceived += (sender, ev) =>
{
    if (!(ev.Targets.Any(x => x is IrcChannel && x.Name == channel) && ev.Source is IrcUser))
    {
        return; // Ignore
    }

    var witnessCommandMatch = witnessCommand.Match(ev.Text);
    if (witnessCommandMatch.Success)
    {
        var data = witnessCommandMatch.Groups["data"];
        var user = ev.Source.Name;  // == nickname
        var hashBytes = sha256.ComputeHash(Encoding.UTF8.GetBytes($"{user}\0{data}"));
        var hash = ByteString.CopyFrom(hashBytes);
        eventUsers[hash] = user;
        streamsClient.WitnessEvent(new()
        {
            EventId = hash
        });
    }

    var helpCommandMatch = witnessCommand.Match(ev.Text);
    if (helpCommandMatch.Success)
    {
        var user = ev.Source.Name;
        var nick = helpCommandMatch.Groups["nickname"].Value;
        if (nick == ircClient.LocalUser.NickName)
        {
            var ircChannel = ircClient.Channels.First(x => x.Name == channel);
            ircClient.LocalUser.SendMessage(ircChannel, String.Format(witnessCommandHelpText, user));
        }
    }

    var witnessReplyMatch = witnessReply.Match(ev.Text);
    if (witnessReplyMatch.Success)
    {
        var hashHex = witnessCommandMatch.Groups["eventId"].Value;
        var hash = ByteString.CopyFrom(Convert.FromHexString(hashHex));
        replyTimers.AddOrUpdate(hash, _ => null, (_, timer) => { timer?.Dispose(); return null; });
    }
};

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

var validatedEvents = streamsClient.ValidatedEvents(new()
{
    FromLatest = true,
});
var byteArrayComparer = new ByteArrayComparer();

await foreach (var events in validatedEvents.ResponseStream.ReadAllAsync())
{
    var ircChannel = ircClient.Channels.First(x => x.Name == channel);
    var channelClients = ircChannel.Users.Select(x => x.User).Where(x => trustedClientRealName.Match(x.RealName).Success).ToList();
    foreach (var @event in events.Events)
    {
        var hash = @event.EventId;
        var orderedChannelClients = channelClients.OrderBy(x => sha256.ComputeHash(hash.ToByteArray().Concat(Encoding.UTF8.GetBytes(x.NickName)).ToArray()), byteArrayComparer);
        var ownNumber = orderedChannelClients.TakeWhile(x => !(x is IrcLocalUser)).Count();
        var timeToWait = SlotToTime(ownNumber);
        replyTimers.AddOrUpdate(hash, _ => new Timer(EventTimerCallback, hash, timeToWait, Timeout.InfiniteTimeSpan), (_, timer) => timer);
    }
}

void EventTimerCallback(object? hashO)
{
    var ircChannel = ircClient.Channels.First(x => x.Name == channel);
    var hash = (ByteString)hashO!;
    replyTimers.AddOrUpdate(hash, _ => null, (_, timer) => { timer?.Dispose(); return null; });
    var user = eventUsers[hash];
    ircClient.LocalUser.SendMessage(ircChannel, String.Format(witnessReplyFormat, hash, user));
}

TimeSpan SlotToTime(int slot)
{
    return TimeSpan.FromSeconds(Math.Sqrt(slot * 1.0f) * 1.0f);
}

public class ByteArrayComparer : IComparer<byte[]>
{
    public int Compare(byte[]? x, byte[]? y)
    {
        int result = (x == null).CompareTo(y == null);
        if (result != 0) return result;
        result = x!.Length.CompareTo(y!.Length);
        if (result != 0) return result;
        for (int index = 0; index < x.Length; index++)
        {
            result = x[index].CompareTo(y[index]);
            if (result != 0) return result;
        }
        return 0;
    }
}
