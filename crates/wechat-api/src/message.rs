//! WeChat message data structures and XML (de)serialization.
//!
//! WeChat callbacks arrive as XML. This module defines strongly-typed
//! structs for the most common message types and provides builders for
//! generating reply XML.

use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Incoming message parsed from the WeChat callback XML body.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename = "xml")]
pub struct WechatMessage {
    /// Developer's WeChat ID (the official account's original ID, `gh_xxx`).
    #[serde(rename = "ToUserName")]
    pub to_user_name: String,

    /// Sender's openid.
    #[serde(rename = "FromUserName")]
    pub from_user_name: String,

    /// Message creation time (unix timestamp).
    #[serde(rename = "CreateTime")]
    pub create_time: i64,

    /// Message type: `text`, `image`, `voice`, `video`, `location`, `link`,
    /// or `event`.
    #[serde(rename = "MsgType")]
    pub msg_type: String,

    /// Text content (only for `text` messages).
    #[serde(rename = "Content", default)]
    pub content: Option<String>,

    /// Message ID (not present for event messages).
    #[serde(rename = "MsgId", default)]
    pub msg_id: Option<i64>,

    /// Image URL (only for `image` messages).
    #[serde(rename = "PicUrl", default)]
    pub pic_url: Option<String>,

    /// Media ID (for image / voice / video messages).
    #[serde(rename = "MediaId", default)]
    pub media_id: Option<String>,

    /// Event type (only for `event` messages): `subscribe`, `unsubscribe`,
    /// `CLICK`, `VIEW`, etc.
    #[serde(rename = "Event", default)]
    pub event: Option<String>,

    /// Event key (for menu `CLICK` events).
    #[serde(rename = "EventKey", default)]
    pub event_key: Option<String>,

    /// Recognition result (for voice messages with `Recognition` enabled).
    #[serde(rename = "Recognition", default)]
    pub recognition: Option<String>,

    /// Location X (latitude, for `location` messages).
    #[serde(rename = "Location_X", default)]
    pub location_x: Option<f64>,

    /// Location Y (longitude, for `location` messages).
    #[serde(rename = "Location_Y", default)]
    pub location_y: Option<f64>,

    /// Map scale (for `location` messages).
    #[serde(rename = "Scale", default)]
    pub scale: Option<i32>,

    /// Location label (for `location` messages).
    #[serde(rename = "Label", default)]
    pub label: Option<String>,

    /// Title (for `link` messages).
    #[serde(rename = "Title", default)]
    pub title: Option<String>,

    /// Description (for `link` messages).
    #[serde(rename = "Description", default)]
    pub description: Option<String>,

    /// URL (for `link` messages).
    #[serde(rename = "Url", default)]
    pub url: Option<String>,
}

impl WechatMessage {
    /// Convenience: is this a text message?
    pub fn is_text(&self) -> bool {
        self.msg_type == "text"
    }

    /// Convenience: is this an event message?
    pub fn is_event(&self) -> bool {
        self.msg_type == "event"
    }

    /// Convenience: is this a subscribe event?
    pub fn is_subscribe(&self) -> bool {
        self.is_event() && self.event.as_deref() == Some("subscribe")
    }

    /// The text content trimmed, or empty string if absent.
    pub fn text_content(&self) -> &str {
        self.content.as_deref().unwrap_or("").trim()
    }
}

/// Parse an XML string into a [`WechatMessage`].
pub fn parse_message(xml: &str) -> Result<WechatMessage, crate::error::WechatError> {
    quick_xml::de::from_str(xml).map_err(|e| crate::error::WechatError::XmlParse(e.to_string()))
}

/// A minimal struct used to extract only `ToUserName` for account routing
/// before the full message is parsed / decrypted.
#[derive(Debug, Deserialize)]
#[serde(rename = "xml")]
pub struct BasicMessage {
    #[serde(rename = "ToUserName")]
    pub to_user_name: String,
}

/// Parse just the `ToUserName` field from the XML body.
pub fn parse_basic(xml: &str) -> Result<BasicMessage, crate::error::WechatError> {
    quick_xml::de::from_str(xml).map_err(|e| crate::error::WechatError::XmlParse(e.to_string()))
}

// ── Reply builders ──────────────────────────────────────────────────────────

/// Build a text reply XML string.
///
/// `to_user` is the sender's openid, `from_user` is the official account's
/// original ID. (These are swapped relative to the incoming message.)
pub fn build_text_reply(to_user: &str, from_user: &str, content: &str) -> String {
    let ts = Utc::now().timestamp();
    format!(
        r#"<xml><ToUserName><![CDATA[{to_user}]]></ToUserName><FromUserName><![CDATA[{from_user}]]></FromUserName><CreateTime>{ts}</CreateTime><MsgType><![CDATA[text]]></MsgType><Content><![CDATA[{content}]]></Content></xml>"#
    )
}

/// Build an image reply XML string (requires a pre-uploaded `media_id`).
pub fn build_image_reply(to_user: &str, from_user: &str, media_id: &str) -> String {
    let ts = Utc::now().timestamp();
    format!(
        r#"<xml><ToUserName><![CDATA[{to_user}]]></ToUserName><FromUserName><![CDATA[{from_user}]]></FromUserName><CreateTime>{ts}</CreateTime><MsgType><![CDATA[image]]></MsgType><Image><MediaId><![CDATA[{media_id}]]></MediaId></Image></xml>"#
    )
}

/// Build a news (link card) reply XML string.
pub fn build_news_reply(
    to_user: &str,
    from_user: &str,
    title: &str,
    description: &str,
    pic_url: &str,
    url: &str,
) -> String {
    let ts = Utc::now().timestamp();
    format!(
        r#"<xml><ToUserName><![CDATA[{to_user}]]></ToUserName><FromUserName><![CDATA[{from_user}]]></FromUserName><CreateTime>{ts}</CreateTime><MsgType><![CDATA[news]]></MsgType><ArticleCount>1</ArticleCount><Articles><item><Title><![CDATA[{title}]]></Title><Description><![CDATA[{description}]]></Description><PicUrl><![CDATA[{pic_url}]]></PicUrl><Url><![CDATA[{url}]]></Url></item></Articles></xml>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TEXT_XML: &str = r#"<xml><ToUserName><![CDATA[gh_abc123]]></ToUserName><FromUserName><![CDATA[oOpenIdXYZ]]></FromUserName><CreateTime>1609459200</CreateTime><MsgType><![CDATA[text]]></MsgType><Content><![CDATA[验证码]]></Content><MsgId>1234567890123456</MsgId></xml>"#;

    #[test]
    fn test_parse_text_message() {
        let msg = parse_message(SAMPLE_TEXT_XML).unwrap();
        assert_eq!(msg.to_user_name, "gh_abc123");
        assert_eq!(msg.from_user_name, "oOpenIdXYZ");
        assert_eq!(msg.msg_type, "text");
        assert_eq!(msg.text_content(), "验证码");
        assert!(msg.is_text());
        assert!(!msg.is_event());
    }

    #[test]
    fn test_parse_basic() {
        let basic = parse_basic(SAMPLE_TEXT_XML).unwrap();
        assert_eq!(basic.to_user_name, "gh_abc123");
    }

    #[test]
    fn test_parse_event_message() {
        let xml = r#"<xml><ToUserName><![CDATA[gh_abc]]></ToUserName><FromUserName><![CDATA[oUser]]></FromUserName><CreateTime>1609459200</CreateTime><MsgType><![CDATA[event]]></MsgType><Event><![CDATA[subscribe]]></Event></xml>"#;
        let msg = parse_message(xml).unwrap();
        assert!(msg.is_event());
        assert!(msg.is_subscribe());
        assert_eq!(msg.event.as_deref(), Some("subscribe"));
    }

    #[test]
    fn test_parse_invalid_xml() {
        assert!(parse_message("not xml").is_err());
    }

    #[test]
    fn test_build_text_reply() {
        let xml = build_text_reply("oUser", "gh_abc", "hello");
        assert!(xml.contains("<ToUserName><![CDATA[oUser]]></ToUserName>"));
        assert!(xml.contains("<Content><![CDATA[hello]]></Content>"));
        assert!(xml.contains("<MsgType><![CDATA[text]]></MsgType>"));
    }

    #[test]
    fn test_build_image_reply() {
        let xml = build_image_reply("oUser", "gh_abc", "media_123");
        assert!(xml.contains("<MsgType><![CDATA[image]]></MsgType>"));
        assert!(xml.contains("<MediaId><![CDATA[media_123]]></MediaId>"));
    }

    #[test]
    fn test_build_news_reply() {
        let xml = build_news_reply("oUser", "gh_abc", "T", "D", "pic", "link");
        assert!(xml.contains("<MsgType><![CDATA[news]]></MsgType>"));
        assert!(xml.contains("<ArticleCount>1</ArticleCount>"));
    }
}
