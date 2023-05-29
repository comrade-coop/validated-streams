using System.Collections.Concurrent;
using Google.Protobuf;

namespace ValidatedStreams.Irc;

public class TrustedClientUserTracker // In practice, would use IPFS for this
{
    ConcurrentDictionary<ByteString, string> users = new();

    public void SetOriginUser(ByteString hash, string username)
    {
        users[hash] = username;
    }
    public string? GetOriginUser(ByteString hash)
    {
        users.TryGetValue(hash, out var username);
        return username;
    }
}
