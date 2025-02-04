//! Synchronous HTTP interactions

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use log::warn;
use serde_json::{json, Value};

use crate::bodies::OptionalBody;
use crate::content_types::ContentType;
use crate::interaction::Interaction;
use crate::json_utils::json_to_string;
use crate::matchingrules::MatchingRules;
use crate::message::Message;
use crate::provider_states::ProviderState;
use crate::sync_interaction::RequestResponseInteraction;
use crate::v4::async_message::AsynchronousMessage;
use crate::v4::http_parts::{HttpRequest, HttpResponse};
use crate::v4::interaction::V4Interaction;
use crate::v4::sync_message::SynchronousMessages;
use crate::v4::V4InteractionType;

/// V4 HTTP Interaction Type
#[derive(Debug, Clone, Eq)]
pub struct SynchronousHttp {
  /// Interaction ID. This will only be set if the Pact file was fetched from a Pact Broker
  pub id: Option<String>,
  /// Unique key for this interaction
  pub key: Option<String>,
  /// A description for the interaction. Must be unique within the Pact file
  pub description: String,
  /// Optional provider states for the interaction.
  /// See https://docs.pact.io/getting_started/provider_states for more info on provider states.
  pub provider_states: Vec<ProviderState>,
  /// Request of the interaction
  pub request: HttpRequest,
  /// Response of the interaction
  pub response: HttpResponse,
  /// Annotations and comments associated with this interaction
  pub comments: HashMap<String, Value>,

  /// If this interaction is pending. Pending interactions will never fail the build if they fail
  pub pending: bool
}

impl SynchronousHttp {
  fn calc_hash(&self) -> String {
    let mut s = DefaultHasher::new();
    self.hash(&mut s);
    format!("{:x}", s.finish())
  }

  /// Creates a new version with a calculated key
  pub fn with_key(&self) -> SynchronousHttp {
    SynchronousHttp {
      key: Some(self.calc_hash()),
      .. self.clone()
    }
  }

  /// Parse the JSON into a SynchronousHttp interaction
  pub fn from_json(json: &Value, index: usize) -> anyhow::Result<SynchronousHttp> {
    if json.is_object() {
      let id = json.get("_id").map(|id| json_to_string(id));
      let key = json.get("key").map(|id| json_to_string(id));
      let description = match json.get("description") {
        Some(v) => match *v {
          Value::String(ref s) => s.clone(),
          _ => v.to_string()
        },
        None => format!("Interaction {}", index)
      };
      let comments = match json.get("comments") {
        Some(v) => match v {
          Value::Object(map) => map.iter()
            .map(|(k, v)| (k.clone(), v.clone())).collect(),
          _ => {
            warn!("Interaction comments must be a JSON Object, but received {}. Ignoring", v);
            Default::default()
          }
        },
        None => Default::default()
      };
      let provider_states = ProviderState::from_json(json);
      let request = json.get("request").cloned().unwrap_or_default();
      let response = json.get("response").cloned().unwrap_or_default();
      Ok(SynchronousHttp {
        id,
        key,
        description,
        provider_states,
        request: HttpRequest::from_json(&request)?,
        response: HttpResponse::from_json(&response)?,
        comments,
        pending: json.get("pending")
          .map(|value| value.as_bool().unwrap_or_default()).unwrap_or_default()
      })
    } else {
      Err(anyhow!("Expected a JSON object for the interaction, got '{}'", json))
    }
  }
}

impl V4Interaction for SynchronousHttp {
  fn to_json(&self) -> Value {
    let mut json = json!({
      "type": V4InteractionType::Synchronous_HTTP.to_string(),
      "key": self.key.clone().unwrap_or_else(|| self.calc_hash()),
      "description": self.description.clone(),
      "request": self.request.to_json(),
      "response": self.response.to_json(),
      "pending": self.pending
    });

    if !self.provider_states.is_empty() {
      let map = json.as_object_mut().unwrap();
      map.insert("providerStates".to_string(), Value::Array(
        self.provider_states.iter().map(|p| p.to_json()).collect()));
    }

    if !self.comments.is_empty() {
      let map = json.as_object_mut().unwrap();
      map.insert("comments".to_string(), self.comments.iter()
        .map(|(k, v)| (k.clone(), v.clone())).collect());
    }

    json
  }

  fn to_super(&self) -> &dyn Interaction {
    self
  }

  fn key(&self) -> Option<String> {
    self.key.clone()
  }

  fn boxed_v4(&self) -> Box<dyn V4Interaction> {
    Box::new(self.clone())
  }

  fn comments(&self) -> HashMap<String, Value> {
    self.comments.clone()
  }

  fn comments_mut(&mut self) -> &mut HashMap<String, Value> {
    &mut self.comments
  }

  fn v4_type(&self) -> V4InteractionType {
    V4InteractionType::Synchronous_HTTP
  }
}

impl Interaction for SynchronousHttp {
  fn type_of(&self) -> String {
    format!("V4 {}", self.v4_type())
  }

  fn is_request_response(&self) -> bool {
    true
  }

  fn as_request_response(&self) -> Option<RequestResponseInteraction> {
    Some(RequestResponseInteraction {
      id: self.id.clone(),
      description: self.description.clone(),
      provider_states: self.provider_states.clone(),
      request: self.request.as_v3_request(),
      response: self.response.as_v3_response()
    })
  }

  fn is_message(&self) -> bool {
    false
  }

  fn as_message(&self) -> Option<Message> {
    None
  }

  fn id(&self) -> Option<String> {
    self.id.clone()
  }

  fn description(&self) -> String {
    self.description.clone()
  }

  fn provider_states(&self) -> Vec<ProviderState> {
    self.provider_states.clone()
  }

  fn contents(&self) -> OptionalBody {
    self.response.body.clone()
  }

  fn contents_for_verification(&self) -> OptionalBody {
    self.response.body.clone()
  }

  fn content_type(&self) -> Option<ContentType> {
    self.response.content_type()
  }

  fn is_v4(&self) -> bool {
    true
  }

  fn as_v4(&self) -> Option<Box<dyn V4Interaction>> {
    Some(self.boxed_v4())
  }

  fn as_v4_http(&self) -> Option<SynchronousHttp> {
    Some(self.clone())
  }

  fn as_v4_async_message(&self) -> Option<AsynchronousMessage> {
    None
  }

  fn as_v4_sync_message(&self) -> Option<SynchronousMessages> {
    None
  }

  fn boxed(&self) -> Box<dyn Interaction + Send> {
    Box::new(self.clone())
  }

  fn arced(&self) -> Arc<dyn Interaction + Send> {
    Arc::new(self.clone())
  }

  fn thread_safe(&self) -> Arc<Mutex<dyn Interaction + Send + Sync>> {
    Arc::new(Mutex::new(self.clone()))
  }

  fn matching_rules(&self) -> Option<MatchingRules> {
    None
  }

  fn pending(&self) -> bool {
    self.pending
  }
}

impl Default for SynchronousHttp {
  fn default() -> Self {
    SynchronousHttp {
      id: None,
      key: None,
      description: "Synchronous/HTTP Interaction".to_string(),
      provider_states: vec![],
      request: HttpRequest::default(),
      response: HttpResponse::default(),
      comments: Default::default(),
      pending: false
    }
  }
}

impl PartialEq for SynchronousHttp {
  fn eq(&self, other: &Self) -> bool {
    self.description == other.description && self.provider_states == other.provider_states &&
      self.request == other.request && self.response == other.response &&
      self.pending == other.pending
  }
}

impl Hash for SynchronousHttp {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.description.hash(state);
    self.provider_states.hash(state);
    self.request.hash(state);
    self.response.hash(state);
    self.pending.hash(state);
  }
}

impl Display for SynchronousHttp {
  fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
    let pending = if self.pending { " [PENDING]" } else { "" };
    write!(f, "V4 Http Interaction{} ( id: {:?}, description: \"{}\", provider_states: {:?}, request: {}, response: {} )",
           pending, self.id, self.description, self.provider_states, self.request, self.response)
  }
}

#[cfg(test)]
mod tests {

}
