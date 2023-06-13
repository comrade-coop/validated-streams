# Irc Sample

To demonstrate an more realistic usecase of Validated Streams, we have set up an example of a validators listening on an IRC channel, submitting events from it to a blockchain, and reporting back to users when those events have been finalized. As it is, extending the example to support a token economics or to verify user identities is left as an exercise to the reader.

## Running the example

Running the example:

1. Build the necessary docker images:
    ```
    ./scripts/run-example.sh build --irc-sample
    ```
2. Start the local network of validators, trusted clients, and an IRC server:
    ```
    ./scripts/run-example.sh start --irc-sample
    ```
3. Connect to the local IRC server at [`localhost:6667`](irc://localhost:6667/validated-stream) (non-TLS), join `#validated-stream` and send a message. Sample interaction:
    ```
    * Now talking on #validated-stream
    <user> bot-bob: help
    <bot-bob> user: !w[itness] <data> -- create and witness a validated-streams event
    <user> !w this is a test event for the README
    <bot-charlie> user: witnessing BE807ED3F92D7C8228302829F829B827E2F7C8338B17A736CAF8AF18403E68F1...
    <bot-charlie> user: BE807ED3F92D7C8228302829F829B827E2F7C8338B17A736CAF8AF18403E68F1 validated!
    ```

    Empirical testing shows that events are validated (finalized) in roughly ~16 seconds by the sample network. It is plausible that tuning the node configuration could produce faster finalizations.
4. Stop the network when you are finished:
    ```
    ./scripts/run-example.sh stop --irc-sample
    ```

## Architecture

The goal of this example is to observe the real-world events of a user sending a specially-formatted text message on a specific IRC channel, then witness those on a Validated Streams chain. Drawn as an architectural diagram, our example would resemble the following:

![A diagram showing a user sending a message to an IRC server which is then received by multiple trusted clients, which then each forward it to its respective validator node as a witnessed event. validated-streams-irc-trustedclient-witness.drawio.png](https://user-images.githubusercontent.com/5276727/242298902-6b5bc399-8b1f-4d7f-9f5a-b58a0b4f17ae.png)

Since this is a toy example, we won't delve into what happens on the chain after the event is received. (Perhaps it is the entry point to a smart contract that later interprets the messages as votes from DAO members or perhaps it is all just a guestbook where visitors leave notes.) Instead, what we would do is wait for a confirmation that the message has been irreversibly stored in the chain and relay that back to the user. Conceptually, inversing the diagram above:

![The same diagram, modified to show the validator nodes finalizing a block and sending it to the trusted clients listening for validated events, who then attempt to arrange for one of them to message the user back. validated-streams-irc-trustedclient-validate.drawio.png](https://user-images.githubusercontent.com/5276727/242298894-636d19d7-3220-4c92-8c55-b81bb35de89a.png)

At this point, the user will have confirmation that their message has been stored on the Validated Streams chain.

