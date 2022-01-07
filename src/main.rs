use reqwest::blocking::Client;
use serde::Deserialize;
use std::io::{stdout, BufWriter, StdoutLock, Write};

/// Non-configurable static values for the Slack API
static CONVERSATION_HISTORY_ENDPOINT: &str = "https://slack.com/api/conversations.history";
static RESPONSE_MESSAGE_LIMIT: i16 = 1000;

/// Configurable values from the environment
#[derive(Deserialize)]
struct Configuration {
    api_token: String,
    channel: String,
}

/// Slack-specific API responses
#[derive(Debug, Deserialize)]
struct SlackResponse {
    ok: bool,
    #[serde(default)]
    messages: Vec<serde_json::Value>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    response_metadata: Option<SlackResponseMetadata>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SlackResponseMetadata {
    next_cursor: String,
}

/// Stream a set of JSON messages in memory to a writer
fn write_messages(
    out: &mut BufWriter<StdoutLock>,
    messages: Vec<serde_json::Value>,
    has_more: bool,
) -> anyhow::Result<()> {
    let mut messages = messages.into_iter().peekable();

    while let Some(message) = messages.next() {
        serde_json::to_writer(out.by_ref(), &message)?;

        if messages.peek().is_some() || has_more {
            out.write_all(b",")?;
        }
    }

    Ok(())
}

/// Stream the entire conversation history to stdout
fn main() -> anyhow::Result<()> {
    // generate the configuration
    let configuration: Configuration = envy::from_env()?;

    // make an initial request to check configured values
    let client = Client::new();

    let response: SlackResponse = client
        .get(CONVERSATION_HISTORY_ENDPOINT)
        .query(&[
            ("channel", &configuration.channel),
            ("limit", &RESPONSE_MESSAGE_LIMIT.to_string()),
        ])
        .bearer_auth(&configuration.api_token)
        .send()?
        .json()?;

    if !response.ok {
        let error = response.error.unwrap_or_else(|| "Unknown".into());

        return Err(anyhow::anyhow!(
            "Error fetching data from the Slack API: {}",
            error
        ));
    }

    // set up exclusive access to stdout
    let stdout = stdout();
    let mut out = BufWriter::new(stdout.lock());

    // generate a single array of messages
    out.write_all(b"[")?;

    let mut has_more = response.has_more;
    let mut next_cursor = response
        .response_metadata
        .map(|metadata| metadata.next_cursor);

    write_messages(&mut out, response.messages, has_more)?;

    while has_more {
        let cursor = next_cursor.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Error fetching additional data: Slack API response missing cursor")
        })?;

        let response: SlackResponse = client
            .get(CONVERSATION_HISTORY_ENDPOINT)
            .query(&[
                ("channel", &configuration.channel),
                ("limit", &RESPONSE_MESSAGE_LIMIT.to_string()),
                ("cursor", cursor),
            ])
            .bearer_auth(&configuration.api_token)
            .send()?
            .json()?;

        has_more = response.has_more;
        next_cursor = response
            .response_metadata
            .map(|metadata| metadata.next_cursor);

        write_messages(&mut out, response.messages, has_more)?;
    }

    out.write_all(b"]")?;

    Ok(())
}
