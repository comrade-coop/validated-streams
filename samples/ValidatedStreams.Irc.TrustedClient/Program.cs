using Grpc.Net.Client;
using IrcDotNet;
using Polly;

namespace ValidatedStreams.Irc;

static public class Program
{
    static public async Task Main(string[] arguments)
    {
        var validatedStreams = arguments[0];
        var ircServer = arguments[1];
        var botChannel = arguments[2];
        var botNickname = arguments[3];

        using var validatedStreamsChannel = GrpcChannel.ForAddress(validatedStreams);
        var validatedStreamsClient = new Streams.StreamsClient(validatedStreamsChannel);

        var ircClient = await Policy.Handle<TimeoutException>()
            .WaitAndRetryAsync(5, attempt => TimeSpan.FromSeconds(attempt))
            .ExecuteAsync(() => StartIrcClient(new Uri(ircServer), botNickname));
        var ircChannel = await Policy.HandleResult<IrcChannel?>(x => x == null)
            .WaitAndRetryAsync(5, attempt => TimeSpan.FromSeconds(attempt))
            .ExecuteAsync(() => Task.FromResult(JoinIrcChannel(ircClient, botChannel)));

        var userTracker = new TrustedClientUserTracker();

        var bot = new TrustedClientBot(ircChannel!, validatedStreamsClient, userTracker);

        await bot.Run();
    }

    static public async Task<IrcClient> StartIrcClient(Uri ircUri, string botNickname)
    {
        var registrationInfo = new IrcUserRegistrationInfo()
        {
            NickName = botNickname,
            UserName = string.Format(TrustedClientBot.UsernameFormat.Item2, botNickname),
            RealName = "Validated Streams bot"
        };
        var ircClient = new StandardIrcClient();
        ircClient.FloodPreventer = new IrcStandardFloodPreventer(4, 2000);
        Console.WriteLine("Connecting to '{0}'", ircUri);

        using (var registeredSemaphore = new SemaphoreSlim(0, 1))
        {
            ircClient.Registered += (sender, e2) => registeredSemaphore.Release();

            ircClient.Connect(ircUri, registrationInfo);
            if (!await registeredSemaphore.WaitAsync(10000))
            {
                ircClient.Dispose();
                throw new TimeoutException("Connection timed out.");
            }
        }

        Console.WriteLine("Connected to '{0}'", ircUri);

        // ircClient.Disconnected += IrcClient_Disconnected;

        return ircClient;
    }

    static public IrcChannel? JoinIrcChannel(IrcClient ircClient, string channelName)
    {
        Console.Write("Joining channel '{0}'", channelName);

        ircClient.Channels.Join(channelName);

        return ircClient.Channels.FirstOrDefault(x => x.Name == channelName);
    }
}
