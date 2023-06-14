using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using Google.Protobuf;
using Grpc.Core;
using IrcDotNet;
using Polly;

namespace ValidatedStreams.Irc;

/// A class encapsulating the bot's main loop of operations.
public class TrustedClientBot
{
    /// A help command sent at the bot.
    public static Regex HelpCommandRegex = new Regex(@"^(?<nickname>[^!@: ]+).+ help.*$");
    /// The witness command sent to all bots in the channel.
    public static Regex WitnessCommandRegex = new Regex(@"^!w(?:itness)? (?<data>.+)$");
    public static string WitnessCommandHelpText = "{0}: !w[itness] <data> -- create and witness a validated-streams event";
    /// The immediate reply we sent when receiving a witness command.
    static (Regex, string) WitnessReply = (new Regex(@"^(?<nickname>[^!@: ]+): witnessing (?<eventId>[A-Z0-9]+)\.\.\.$"), "{1}: witnessing {0}...");
    /// The confirmation reply we sent when receiving the validated event back.
    static (Regex, string) ValidateReply = (new Regex(@"^(?<nickname>[^!@: ]+|<unknown user>): (?<eventId>[A-Z0-9]+) validated!$"), "{1}: {0} validated!");
    /// The format for the bot's username, so that we can find the other bots in the user list.
    public static (Regex, string) UsernameFormat = (new Regex(@"vs-.*"), "vs-{0}");

    private IrcChannel IrcChannel;
    private Streams.StreamsClient ValidatedStreamsClient;
    private TrustedClientUserTracker UserTracker;

    public TrustedClientBot(IrcChannel ircChannel, Streams.StreamsClient validatedStreamsClient, TrustedClientUserTracker userTracker)
    {
        IrcChannel = ircChannel;
        ValidatedStreamsClient = validatedStreamsClient;
        UserTracker = userTracker;
    }

    public async Task Run()
    {
        HandleIncommingMessages();

        await ForwardValidatedEvents();
    }

    public void HandleIncommingMessages()
    {
        var witnessReplier = new TrustedClientReplier(IrcChannel, WitnessReply, UsernameFormat, UserTracker);

        IrcChannel.MessageReceived += (sender, ev) =>
        {
            try
            {
                Console.WriteLine("Received message {0}", ev.Text);
                var witnessCommandMatch = WitnessCommandRegex.Match(ev.Text);
                if (witnessCommandMatch.Success)
                {
                    var data = witnessCommandMatch.Groups["data"];
                    var user = ev.Source.Name;  // == nickname
                    using var sha256 = SHA256.Create();
                    var hashBytes = sha256.ComputeHash(Encoding.UTF8.GetBytes($"{user}\0{data}"));
                    var hash = ByteString.CopyFrom(hashBytes);
                    Console.WriteLine("Witnessing {0}", Convert.ToHexString(hash.ToByteArray()));
                    ValidatedStreamsClient.WitnessEvent(new()
                    {
                        EventId = hash
                    });
                    UserTracker.SetOriginUser(hash, user);
                    witnessReplier.SendReply(hash);
                }

                var helpCommandMatch = HelpCommandRegex.Match(ev.Text);
                if (helpCommandMatch.Success)
                {
                    var user = ev.Source.Name;
                    var nick = helpCommandMatch.Groups["nickname"].Value;
                    if (nick == IrcChannel.Client.LocalUser.NickName)
                    {
                        IrcChannel.Client.LocalUser.SendMessage(IrcChannel, String.Format(WitnessCommandHelpText, user));
                    }
                }
            }
            catch (Exception e)
            {
                Console.WriteLine(e);
                throw;
            }
        };
    }

    public async Task ForwardValidatedEvents()
    {
        var validateReplier = new TrustedClientReplier(IrcChannel, ValidateReply, UsernameFormat, UserTracker);

        await Policy.Handle<RpcException>(ex => ex.Status.StatusCode == StatusCode.Unavailable)
            .WaitAndRetryForeverAsync(attempt => TimeSpan.FromSeconds(attempt))
            .ExecuteAsync(async () =>
            {
                var validatedEvents = ValidatedStreamsClient.ValidatedEvents(new()
                {
                    FromLatest = true,
                });

                await foreach (var events in validatedEvents.ResponseStream.ReadAllAsync())
                {
                    foreach (var @event in events.Events)
                    {
                        var hash = @event.EventId;
                        Console.WriteLine("Validating {0}", Convert.ToHexString(hash.ToByteArray()));
                        validateReplier.SendReply(hash);
                    }
                }
            });
    }
}
