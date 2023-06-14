FROM mcr.microsoft.com/dotnet/sdk:6.0 AS build
WORKDIR /src
COPY proto proto
WORKDIR /src/samples/irc
COPY samples/irc/ValidatedStreams.Irc.TrustedClient.csproj .
RUN dotnet restore "ValidatedStreams.Irc.TrustedClient.csproj"
COPY samples/irc/ .
RUN dotnet publish "ValidatedStreams.Irc.TrustedClient.csproj" --no-restore -o /app/publish # -c Release

FROM mcr.microsoft.com/dotnet/runtime:6.0
WORKDIR /app
COPY --from=build /app/publish .
ENTRYPOINT ["dotnet", "ValidatedStreams.Irc.TrustedClient.dll"]
