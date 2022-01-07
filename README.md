# `slump`

## "slack dump"

Dump the message history of a Slack channel to a JSON file.

### Installation

1. Make sure that you have a [Rust toolchain installed](https://rustup.rs/)
2. Clone this repo
3. Run this thing with `cargo run` or through compilation

### How to use

1. Make sure that you have a Slack App with a valid token with `conversation.history` permissions
2. Get the channel ID for the channel you would like to make a dump of
3. Include both as environment variables (`API_TOKEN` and `CHANNEL`, respectively)
4. Messages are streamed as valid JSON to `stdout`. The easiest way to back up a channel, then, is
   through redirection to a `.json` file, e.g.:

   ```bash
   API_TOKEN=<your-slack-api-token> CHANNEL=<your-slack-channel-ID> cargo run > dump.json
   ```
