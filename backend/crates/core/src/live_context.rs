//! Live data acquisition for missions.
//!
//! LLM providers have no real-time knowledge, so objectives that ask about
//! the present ("exact price of bitcoin", "weather in Chennai", a pasted URL)
//! are answered by first pulling the facts from keyless public sources and
//! injecting them into the reasoning prompt as ground truth. Every fetch goes
//! through [`crate::nexus::safe_fetch::safe_fetch_html`], which blocks private
//! hosts and caps body size, and runs on a dedicated IO thread.

use chrono::Utc;
use serde_json::Value;

/// Upper bound for the injected context block so it never crowds out the
/// user's actual objective in the provider prompt.
const CONTEXT_CHAR_LIMIT: usize = 2500;
/// At most this many pasted URLs are fetched per mission.
const MAX_URL_FETCHES: usize = 2;
const URL_EXCERPT_CHARS: usize = 1200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveIntent {
    CryptoPrice(Vec<&'static Coin>),
    FiatRate { base: String, quote: String },
    Weather(String),
    TimeNow,
    FetchUrls(Vec<String>),
    FreshFacts,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Coin {
    pub gecko_id: &'static str,
    pub aliases: &'static [&'static str],
}

static COINS: &[Coin] = &[
    Coin { gecko_id: "bitcoin", aliases: &["bitcoin", "btc"] },
    Coin { gecko_id: "ethereum", aliases: &["ethereum", "ether", "eth"] },
    Coin { gecko_id: "solana", aliases: &["solana", "sol"] },
    Coin { gecko_id: "dogecoin", aliases: &["dogecoin", "doge"] },
    Coin { gecko_id: "ripple", aliases: &["ripple", "xrp"] },
    Coin { gecko_id: "binancecoin", aliases: &["bnb", "binance coin"] },
    Coin { gecko_id: "cardano", aliases: &["cardano", "ada"] },
    Coin { gecko_id: "litecoin", aliases: &["litecoin", "ltc"] },
    Coin { gecko_id: "polkadot", aliases: &["polkadot"] },
    Coin { gecko_id: "tron", aliases: &["tron", "trx"] },
];

const FIAT: &[&str] = &[
    "usd", "eur", "inr", "gbp", "jpy", "aud", "cad", "chf", "cny", "sgd", "aed",
];

const VALUE_WORDS: &[&str] = &[
    "price", "worth", "value", "cost", "rate", "much", "exact", "current", "today", "now",
    "market",
];

const FRESH_WORDS: &[&str] = &[
    "latest", "news", "today", "current", "currently", "now", "recent", "this week",
    "right now", "live",
];

fn contains_word(lower: &str, word: &str) -> bool {
    lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|token| token == word)
        || (word.contains(' ') && lower.contains(word))
}

/// Decides which live sources the objective needs. Returns an empty list for
/// timeless questions so they cost nothing extra.
#[must_use]
pub fn detect_intents(objective: &str) -> Vec<LiveIntent> {
    let lower = objective.to_ascii_lowercase();
    let mut intents = Vec::new();

    let urls: Vec<String> = objective
        .split_whitespace()
        .filter(|token| token.starts_with("http://") || token.starts_with("https://"))
        .take(MAX_URL_FETCHES)
        .map(|token| token.trim_end_matches([')', ']', '.', ',']).to_string())
        .collect();
    if !urls.is_empty() {
        intents.push(LiveIntent::FetchUrls(urls));
    }

    let value_query = VALUE_WORDS.iter().any(|word| contains_word(&lower, word));
    let coins: Vec<&'static Coin> = COINS
        .iter()
        .filter(|coin| coin.aliases.iter().any(|alias| contains_word(&lower, alias)))
        .collect();
    if !coins.is_empty() && value_query {
        intents.push(LiveIntent::CryptoPrice(coins));
    }

    // "usd to inr", "100 eur in gbp"
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    for window in tokens.windows(3) {
        if FIAT.contains(&window[0])
            && matches!(window[1], "to" | "in" | "vs" | "into")
            && FIAT.contains(&window[2])
            && window[0] != window[2]
        {
            intents.push(LiveIntent::FiatRate {
                base: window[0].to_ascii_uppercase(),
                quote: window[2].to_ascii_uppercase(),
            });
            break;
        }
    }

    if let Some(position) = lower.find("weather") {
        let after = &lower[position + "weather".len()..];
        let location = after
            .trim_start()
            .strip_prefix("in ")
            .or_else(|| after.trim_start().strip_prefix("at "))
            .or_else(|| after.trim_start().strip_prefix("for "))
            .map(|rest| {
                rest.split(|c: char| c == '?' || c == '.' || c == ',' || c == '!')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            })
            .filter(|loc| !loc.is_empty() && loc.len() <= 60);
        if let Some(location) = location {
            intents.push(LiveIntent::Weather(location));
        }
    }

    if lower.contains("time now")
        || lower.contains("current time")
        || lower.contains("what time")
        || lower.contains("today's date")
        || lower.contains("what day")
        || lower.contains("date today")
    {
        intents.push(LiveIntent::TimeNow);
    }

    if intents.is_empty() && FRESH_WORDS.iter().any(|word| contains_word(&lower, word)) {
        intents.push(LiveIntent::FreshFacts);
    }
    intents
}

/// Fetches every detected intent (best-effort, each source independent) and
/// formats the findings as a prompt block. `None` when the objective needs no
/// live data or every source failed.
///
/// Blocking: call from a thread that may sleep (the fetches use dedicated IO
/// threads but this function waits for them).
#[must_use]
pub fn gather(objective: &str) -> Option<LiveContext> {
    let intents = detect_intents(objective);
    if intents.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    let mut sources = Vec::new();
    for intent in &intents {
        let (source, result) = match intent {
            LiveIntent::CryptoPrice(coins) => ("coingecko", fetch_crypto(coins)),
            LiveIntent::FiatRate { base, quote } => ("frankfurter", fetch_fiat(base, quote)),
            LiveIntent::Weather(location) => ("wttr.in", fetch_weather(location)),
            LiveIntent::TimeNow => ("local clock", Ok(vec![format!(
                "current UTC time is {}, local device time is {}",
                Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z"),
            )])),
            LiveIntent::FetchUrls(urls) => ("user link", fetch_urls(urls)),
            LiveIntent::FreshFacts => ("duckduckgo", fetch_instant_answer(objective)),
        };
        match result {
            Ok(found) if !found.is_empty() => {
                sources.push(source.to_string());
                for line in found {
                    lines.push(format!("- [{source}] {line}"));
                }
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(source, %error, "live context source failed");
            }
        }
    }
    if lines.is_empty() {
        return None;
    }
    // The ASC-II sensitivity gate scans the objective text; a fetched line
    // mentioning e.g. "wallet" would silently force the mission local, so any
    // such line is dropped rather than risking the user's provider choice.
    lines.retain(|line| {
        let lower = line.to_ascii_lowercase();
        !["password", "private key", "seed phrase", "api key", "credential", "wallet", "secret"]
            .iter()
            .any(|needle| lower.contains(needle))
    });
    if lines.is_empty() {
        return None;
    }
    let mut block = format!(
        "\n\n[LIVE DATA — fetched {} by the local Astra device. Treat these values as current ground truth and use them to answer; cite the source name.]\n",
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    );
    for line in lines {
        if block.chars().count() + line.chars().count() > CONTEXT_CHAR_LIMIT {
            break;
        }
        block.push_str(&line);
        block.push('\n');
    }
    Some(LiveContext { block, sources })
}

#[derive(Debug, Clone)]
pub struct LiveContext {
    /// Ready-to-append prompt block.
    pub block: String,
    /// Source names, for reply footers and audit records.
    pub sources: Vec<String>,
}

fn fetch_json(url: &str) -> Result<Value, String> {
    let body = crate::nexus::safe_fetch::safe_fetch_html(url)?;
    serde_json::from_str(&body).map_err(|error| format!("invalid JSON from {url}: {error}"))
}

fn fetch_crypto(coins: &[&'static Coin]) -> Result<Vec<String>, String> {
    let ids = coins
        .iter()
        .map(|coin| coin.gecko_id)
        .collect::<Vec<_>>()
        .join(",");
    let value = fetch_json(&format!(
        "https://api.coingecko.com/api/v3/simple/price?ids={ids}&vs_currencies=usd,inr,eur&include_24hr_change=true"
    ))?;
    let mut lines = Vec::new();
    for coin in coins {
        if let Some(entry) = value.get(coin.gecko_id) {
            let usd = entry.get("usd").and_then(Value::as_f64);
            let inr = entry.get("inr").and_then(Value::as_f64);
            let eur = entry.get("eur").and_then(Value::as_f64);
            let change = entry.get("usd_24h_change").and_then(Value::as_f64);
            if let Some(usd) = usd {
                lines.push(format!(
                    "{} price: {usd} USD{}{}{}",
                    coin.gecko_id,
                    inr.map(|v| format!(" / {v} INR")).unwrap_or_default(),
                    eur.map(|v| format!(" / {v} EUR")).unwrap_or_default(),
                    change
                        .map(|v| format!(" (24h change {v:+.2}% in USD)"))
                        .unwrap_or_default(),
                ));
            }
        }
    }
    Ok(lines)
}

fn fetch_fiat(base: &str, quote: &str) -> Result<Vec<String>, String> {
    let value = fetch_json(&format!(
        "https://api.frankfurter.dev/v1/latest?base={base}&symbols={quote}"
    ))?;
    let rate = value
        .pointer(&format!("/rates/{quote}"))
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("no {base}->{quote} rate in response"))?;
    let date = value.get("date").and_then(Value::as_str).unwrap_or("today");
    Ok(vec![format!(
        "exchange rate (ECB reference, {date}): 1 {base} = {rate} {quote}"
    )])
}

fn fetch_weather(location: &str) -> Result<Vec<String>, String> {
    let encoded = location.trim().replace(' ', "+");
    let line = crate::nexus::safe_fetch::safe_fetch_html(&format!(
        "https://wttr.in/{encoded}?format=%l:+%C,+%t+(feels+like+%f),+humidity+%h,+wind+%w"
    ))?;
    let line = line.trim().to_string();
    if line.is_empty() || line.len() > 300 {
        return Err("weather service returned no usable report".into());
    }
    Ok(vec![format!("weather report: {line}")])
}

fn fetch_urls(urls: &[String]) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for url in urls.iter().take(MAX_URL_FETCHES) {
        match crate::nexus::safe_fetch::safe_fetch_html(url) {
            Ok(body) => {
                let text = strip_tags(&body);
                let excerpt: String = text.chars().take(URL_EXCERPT_CHARS).collect();
                if !excerpt.trim().is_empty() {
                    lines.push(format!("content of {url}: {}", excerpt.trim()));
                }
            }
            Err(error) => lines.push(format!("could not fetch {url}: {error}")),
        }
    }
    Ok(lines)
}

fn fetch_instant_answer(query: &str) -> Result<Vec<String>, String> {
    let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    let value = fetch_json(&format!(
        "https://api.duckduckgo.com/?q={encoded}&format=json&no_html=1&skip_disambig=1"
    ))?;
    let mut lines = Vec::new();
    for pointer in ["/Answer", "/AbstractText", "/Definition"] {
        if let Some(text) = value.pointer(pointer).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                lines.push(text.trim().chars().take(600).collect());
                break;
            }
        }
    }
    Ok(lines)
}

/// Minimal tag stripper for fetched pages: drops script/style bodies, all
/// tags, and collapses whitespace.
#[must_use]
pub fn strip_tags(html: &str) -> String {
    let mut text = String::with_capacity(html.len() / 2);
    let mut chars = html.char_indices().peekable();
    let lower = html.to_ascii_lowercase();
    let mut skip_until: Option<&str> = None;
    let mut in_tag = false;
    while let Some((index, c)) = chars.next() {
        if let Some(closer) = skip_until {
            if lower[index..].starts_with(closer) {
                skip_until = None;
                for _ in 0..closer.len() - 1 {
                    chars.next();
                }
            }
            continue;
        }
        if c == '<' {
            if lower[index..].starts_with("<script") {
                skip_until = Some("</script>");
            } else if lower[index..].starts_with("<style") {
                skip_until = Some("</style>");
            } else {
                in_tag = true;
            }
            continue;
        }
        if in_tag {
            if c == '>' {
                in_tag = false;
                text.push(' ');
            }
            continue;
        }
        text.push(c);
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitcoin_price_query_detects_crypto_intent() {
        let intents = detect_intents("what is the exact price of bitcoin?");
        assert!(matches!(
            intents.as_slice(),
            [LiveIntent::CryptoPrice(coins)] if coins[0].gecko_id == "bitcoin"
        ));
    }

    #[test]
    fn ticker_alias_and_multiple_coins() {
        let intents = detect_intents("how much are BTC and ETH worth now");
        match intents.as_slice() {
            [LiveIntent::CryptoPrice(coins)] => {
                let ids: Vec<_> = coins.iter().map(|c| c.gecko_id).collect();
                assert_eq!(ids, vec!["bitcoin", "ethereum"]);
            }
            other => panic!("unexpected intents: {other:?}"),
        }
    }

    #[test]
    fn fiat_weather_time_and_urls() {
        assert!(matches!(
            detect_intents("convert 100 usd to inr please").as_slice(),
            [LiveIntent::FiatRate { base, quote }] if base == "USD" && quote == "INR"
        ));
        assert!(matches!(
            detect_intents("what's the weather in chennai today?").as_slice(),
            [LiveIntent::Weather(loc)] if loc == "chennai today"
                || loc == "chennai"
        ));
        assert!(
            detect_intents("what time now in london")
                .contains(&LiveIntent::TimeNow)
        );
        assert!(matches!(
            detect_intents("summarize https://example.com/post.").as_slice(),
            [LiveIntent::FetchUrls(urls)] if urls == &["https://example.com/post"]
        ));
    }

    #[test]
    fn timeless_questions_cost_nothing() {
        assert!(detect_intents("explain how quicksort works").is_empty());
        assert!(detect_intents("write a haiku about rust").is_empty());
    }

    #[test]
    fn fresh_facts_fallback_only_when_nothing_else_matched() {
        assert_eq!(
            detect_intents("latest spacex launch result"),
            vec![LiveIntent::FreshFacts]
        );
        // crypto intent suppresses the generic fallback
        assert!(matches!(
            detect_intents("current bitcoin price").as_slice(),
            [LiveIntent::CryptoPrice(_)]
        ));
    }

    #[test]
    fn strips_tags_and_scripts() {
        let html = "<html><head><style>body{x}</style><script>evil()</script></head>\
                    <body><h1>Title</h1><p>Hello <b>world</b></p></body></html>";
        assert_eq!(strip_tags(html), "Title Hello world");
    }

    #[test]
    #[ignore = "network: hits coingecko and formats a live block"]
    fn gather_bitcoin_price_live() {
        let context = gather("exact price of bitcoin right now").expect("live context");
        assert!(context.sources.contains(&"coingecko".to_string()));
        assert!(context.block.contains("bitcoin price:"));
        println!("{}", context.block);
    }
}
