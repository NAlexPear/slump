use super::Configuration;
use fallible_iterator::FallibleIterator;
use reqwest::blocking::Client;
use serde::Deserialize;

/// Non-configurable static values for the Slack API
static CONVERSATION_HISTORY_ENDPOINT: &str = "https://slack.com/api/conversations.history";
static RESPONSE_MESSAGE_LIMIT: i16 = 1000;

/// Top-level Slack API client for a single channel
pub struct Slack {
    api_token: String,
    channel: String,
    client: Client,
}

impl Slack {
    /// Fetch a single chunk of messages from the conversation history API
    fn get_message_chunk(&self, cursor: Option<&String>) -> anyhow::Result<MessageChunk> {
        let mut request = self
            .client
            .get(CONVERSATION_HISTORY_ENDPOINT)
            .bearer_auth(&self.api_token);

        if let Some(cursor) = cursor {
            request = request.query(&[
                ("channel", &self.channel),
                ("limit", &RESPONSE_MESSAGE_LIMIT.to_string()),
                ("cursor", cursor),
            ]);
        } else {
            request = request.query(&[
                ("channel", &self.channel),
                ("limit", &RESPONSE_MESSAGE_LIMIT.to_string()),
            ]);
        }

        request.send()?.json::<Response>()?.try_into()
    }

    /// Return all of the messages from the conversation history API
    pub fn messages(&self) -> anyhow::Result<Messages> {
        let message_chunk = self.get_message_chunk(None)?;

        Ok(Messages {
            client: self,
            current_chunk: message_chunk,
        })
    }
}

impl From<Configuration> for Slack {
    fn from(configuration: Configuration) -> Self {
        let Configuration { api_token, channel } = configuration;
        let client = Client::new();

        Self {
            api_token,
            channel,
            client,
        }
    }
}

/// Fallible iterator over messages from the Slack API
pub struct Messages<'a> {
    client: &'a Slack,
    current_chunk: MessageChunk,
}

impl<'a> FallibleIterator for Messages<'a> {
    type Item = serde_json::Value;
    type Error = anyhow::Error;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        match self.current_chunk.next() {
            Some(message) => Ok(Some(message)),
            None => match &self.current_chunk {
                MessageChunk::Terminal { .. } => Ok(None),
                MessageChunk::NonTerminal { next_cursor, .. } => {
                    self.current_chunk = self.client.get_message_chunk(Some(next_cursor))?;
                    Ok(self.current_chunk.next())
                }
            },
        }
    }
}

/// Processed chunk of messages from the Slack API
enum MessageChunk {
    NonTerminal {
        messages: std::vec::IntoIter<serde_json::Value>,
        next_cursor: String,
    },
    Terminal {
        messages: std::vec::IntoIter<serde_json::Value>,
    },
}

impl Iterator for MessageChunk {
    type Item = serde_json::Value;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::NonTerminal { messages, .. } => messages.next(),
            Self::Terminal { messages } => messages.next(),
        }
    }
}

impl TryFrom<Response> for MessageChunk {
    type Error = anyhow::Error;

    fn try_from(response: Response) -> Result<Self, Self::Error> {
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
                messages: response.messages.into_iter(),
                next_cursor: metadata.next_cursor,
            }
        } else {
            Self::Terminal {
                messages: response.messages.into_iter(),
            }
        };

        Ok(chunk)
    }
}

/// Slack-specific API responses
#[derive(Debug, Deserialize)]
struct Response {
    ok: bool,
    #[serde(default)]
    messages: Vec<serde_json::Value>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    response_metadata: Option<ResponseMetadata>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponseMetadata {
    next_cursor: String,
}
