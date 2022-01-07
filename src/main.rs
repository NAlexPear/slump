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

/// Processed set of messages from the Slack API
enum MessageChunk {
    NonTerminal {
        messages: Vec<serde_json::Value>,
        next_cursor: String,
    },
    Terminal {
        messages: Vec<serde_json::Value>,
    },
}

impl MessageChunk {
    fn messages(&self) -> impl Iterator<Item = &serde_json::Value> {
        match self {
            Self::NonTerminal { messages, .. } => messages.iter(),
            Self::Terminal { messages } => messages.iter(),
        }
    }
}

impl TryFrom<SlackResponse> for MessageChunk {
    type Error = anyhow::Error;

    fn try_from(response: SlackResponse) -> Result<Self, Self::Error> {
        // guard against general error responses from the API
        if !response.ok {
            let error = response.error.unwrap_or_else(|| "Unknown".into());

            return Err(anyhow::anyhow!(
                "Error fetching data from the Slack API: {}",
                error
            ));
        }

        // guard against invalid cursor values
        let chunk = if response.has_more {
            let metadata = response.response_metadata.ok_or_else(|| {
                anyhow::anyhow!("Error fetching additional data: Slack API response missing cursor")
            })?;

            Self::NonTerminal {
                messages: response.messages,
                next_cursor: metadata.next_cursor,
            }
        } else {
            Self::Terminal {
                messages: response.messages,
            }
        };

        Ok(chunk)
    }
}

/// Stream a chunk of JSON messages in memory to a writer
fn write_message_chunk(
    out: &mut BufWriter<StdoutLock>,
    chunk: &MessageChunk,
) -> anyhow::Result<()> {
    let mut messages = chunk.messages().peekable();

    while let Some(message) = messages.next() {
        serde_json::to_writer(out.by_ref(), &message)?;

        if messages.peek().is_some() || matches!(chunk, MessageChunk::NonTerminal { .. }) {
            out.write_all(b",")?;
        }
    }

    Ok(())
}

/// Fetch a single chunk of messages from the conversation history API
fn get_message_chunk(
    client: &Client,
    configuration: &Configuration,
    cursor: Option<String>,
) -> anyhow::Result<MessageChunk> {
    let mut request = client
        .get(CONVERSATION_HISTORY_ENDPOINT)
        .bearer_auth(&configuration.api_token);

    if let Some(cursor) = cursor {
        request = request.query(&[
            ("channel", &configuration.channel),
            ("limit", &RESPONSE_MESSAGE_LIMIT.to_string()),
            ("cursor", &cursor),
        ]);
    } else {
        request = request.query(&[
            ("channel", &configuration.channel),
            ("limit", &RESPONSE_MESSAGE_LIMIT.to_string()),
        ]);
    }

    request.send()?.json::<SlackResponse>()?.try_into()
}

/// Stream the entire conversation history to stdout
fn main() -> anyhow::Result<()> {
    // generate the configuration
    let configuration: Configuration = envy::from_env()?;

    // make an initial request to check configured values
    let client = Client::new();
    let mut message_chunk = get_message_chunk(&client, &configuration, None)?;

    // set up exclusive access to stdout
    let stdout = stdout();
    let mut out = BufWriter::new(stdout.lock());

    // generate a single array of messages
    out.write_all(b"[")?;

    write_message_chunk(&mut out, &message_chunk)?;

    while let MessageChunk::NonTerminal { next_cursor, .. } = message_chunk {
        message_chunk = get_message_chunk(&client, &configuration, Some(next_cursor))?;
        write_message_chunk(&mut out, &message_chunk)?;
    }

    out.write_all(b"]")?;

    Ok(())
}
