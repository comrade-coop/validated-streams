namespace ValidatedStreams.Irc;

/// Utility class defining a total order over byte arrays.
internal class ByteArrayComparer : IComparer<byte[]>
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
