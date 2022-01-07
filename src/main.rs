use fallible_iterator::FallibleIterator;
use serde::Deserialize;
use slack::Slack;
use std::io::{stdout, BufWriter, Write};

mod slack;

/// Configurable values from the environment
#[derive(Deserialize)]
struct Configuration {
    api_token: String,
    channel: String,
}

/// Stream the entire conversation history to stdout
fn main() -> anyhow::Result<()> {
    // generate the configuration
    let configuration: Configuration = envy::from_env()?;

    // fetch the stream of messages from Slack
    let slack: Slack = configuration.into();
    let mut messages = slack.messages()?.peekable();

    // set up exclusive access to stdout
    let stdout = stdout();
    let mut out = BufWriter::new(stdout.lock());

    // generate a single array of messages
    out.write_all(b"[")?;

    while let Some(message) = messages.next()? {
        serde_json::to_writer(out.by_ref(), &message)?;

        if messages.peek().transpose().is_some() {
            out.write_all(b",")?;
        }
    }

    out.write_all(b"]")?;

    Ok(())
}
