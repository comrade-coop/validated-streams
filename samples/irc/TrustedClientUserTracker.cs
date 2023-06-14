using System.Collections.Concurrent;
using Google.Protobuf;

namespace ValidatedStreams.Irc;

/// A class tracking the user that has submitted a certain event.
/// A more practical implementation would use IPFS for this, but this one is good enough for example purposes.
public class TrustedClientUserTracker
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
