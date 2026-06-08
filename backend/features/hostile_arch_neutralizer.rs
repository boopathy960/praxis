// ─────────────────────────────────────────────────────────────
// Feature: Hostile Architecture Neutralizer (Anti-Enshittification)
// ─────────────────────────────────────────────────────────────
// Detects and neutralizes hostile web design patterns: paywalls,
// dark patterns, login walls, GDPR annoyances, notification spam,
// SEO garbage, ad overload, cookie walls, autoplay, popup hell.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum HostilePattern {
    Paywall,
    DarkPattern,
    LoginWall,
    GdprBanner,
    NotificationSpam,
    SeoGarbage,
    AdOverload,
    CookieWall,
    AutoplayVideo,
    PopupHell,
    InfiniteScroll,
    ForcedNewsletter,
}

impl HostilePattern {
    pub fn severity(&self) -> f64 {
        match self {
            Self::Paywall => 0.9,
            Self::DarkPattern => 0.85,
            Self::LoginWall => 0.7,
            Self::GdprBanner => 0.3,
            Self::NotificationSpam => 0.6,
            Self::SeoGarbage => 0.4,
            Self::AdOverload => 0.75,
            Self::CookieWall => 0.35,
            Self::AutoplayVideo => 0.65,
            Self::PopupHell => 0.8,
            Self::InfiniteScroll => 0.2,
            Self::ForcedNewsletter => 0.5,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Paywall => "Paywall",
            Self::DarkPattern => "Dark Pattern",
            Self::LoginWall => "Login Wall",
            Self::GdprBanner => "GDPR Banner",
            Self::NotificationSpam => "Notification Spam",
            Self::SeoGarbage => "SEO Garbage",
            Self::AdOverload => "Ad Overload",
            Self::CookieWall => "Cookie Wall",
            Self::AutoplayVideo => "Autoplay Video",
            Self::PopupHell => "Popup Hell",
            Self::InfiniteScroll => "Infinite Scroll Trap",
            Self::ForcedNewsletter => "Forced Newsletter",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DetectedThreat {
    pub id: String,
    pub pattern: HostilePattern,
    pub selector: String,
    pub confidence: f64,
    pub severity: f64,
    pub description: String,
    pub neutralization_css: String,
    pub neutralization_js: String,
    pub neutralized: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageScanResult {
    pub url: String,
    pub enshittification_score: f64,
    pub threats: Vec<DetectedThreat>,
    pub clean_css: String,
    pub total_detected: usize,
    pub total_neutralized: usize,
    pub categories: HashMap<String, usize>,
}

pub struct HostileArchNeutralizer {
    pattern_signatures: Vec<PatternSignature>,
    auto_mode: bool,
    total_scans: u64,
    total_threats_detected: u64,
    total_neutralized: u64,
    site_scores: HashMap<String, f64>,
}

struct PatternSignature {
    pattern: HostilePattern,
    html_indicators: Vec<&'static str>,
    css_selectors: Vec<&'static str>,
    class_patterns: Vec<&'static str>,
    neutralize_css: &'static str,
    neutralize_js: &'static str,
}

impl HostileArchNeutralizer {
    pub fn new() -> Self {
        Self {
            pattern_signatures: Self::build_signature_database(),
            auto_mode: true,
            total_scans: 0,
            total_threats_detected: 0,
            total_neutralized: 0,
            site_scores: HashMap::new(),
        }
    }

    /// Scan a page for hostile architecture patterns.
    pub fn scan_page(&mut self, url: &str, html: &str) -> PageScanResult {
        self.total_scans += 1;
        let html_lower = html.to_lowercase();
        let mut threats = Vec::new();
        let mut categories: HashMap<String, usize> = HashMap::new();

        for sig in &self.pattern_signatures {
            let mut confidence: f64 = 0.0;
            let mut matched_selector = String::new();

            // Check HTML content indicators
            for indicator in &sig.html_indicators {
                if html_lower.contains(indicator) {
                    confidence += 0.25;
                    if matched_selector.is_empty() {
                        matched_selector = indicator.to_string();
                    }
                }
            }

            // Check CSS class/id patterns
            for pattern in &sig.class_patterns {
                if html_lower.contains(pattern) {
                    confidence += 0.3;
                    if matched_selector.is_empty() {
                        matched_selector = pattern.to_string();
                    }
                }
            }

            confidence = confidence.min(1.0);

            if confidence >= 0.25 {
                let id = format!("threat_{}_{}", self.total_threats_detected, threats.len());
                let severity = sig.pattern.severity() * confidence;

                threats.push(DetectedThreat {
                    id,
                    pattern: sig.pattern,
                    selector: if matched_selector.is_empty() {
                        sig.css_selectors.first().unwrap_or(&"body").to_string()
                    } else {
                        matched_selector
                    },
                    confidence,
                    severity,
                    description: format!(
                        "{} detected with {:.0}% confidence",
                        sig.pattern.label(),
                        confidence * 100.0
                    ),
                    neutralization_css: sig.neutralize_css.to_string(),
                    neutralization_js: sig.neutralize_js.to_string(),
                    neutralized: self.auto_mode,
                });

                *categories
                    .entry(sig.pattern.label().to_string())
                    .or_insert(0) += 1;
                self.total_threats_detected += 1;
            }
        }

        // Compute enshittification score (0=clean, 100=total garbage)
        let enshittification = if threats.is_empty() {
            0.0
        } else {
            let weighted_sum: f64 = threats.iter().map(|t| t.severity).sum();
            (weighted_sum / threats.len() as f64 * 100.0).min(100.0)
        };

        // Generate combined cleanup CSS
        let clean_css = threats
            .iter()
            .filter(|t| t.neutralized)
            .map(|t| t.neutralization_css.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let neutralized_count = threats.iter().filter(|t| t.neutralized).count();
        self.total_neutralized += neutralized_count as u64;
        self.site_scores.insert(url.to_string(), enshittification);

        PageScanResult {
            url: url.to_string(),
            enshittification_score: enshittification,
            threats: threats.clone(),
            clean_css,
            total_detected: threats.len(),
            total_neutralized: neutralized_count,
            categories,
        }
    }

    /// Toggle auto-neutralization mode.
    pub fn set_auto_mode(&mut self, enabled: bool) {
        self.auto_mode = enabled;
    }

    pub fn is_auto_mode(&self) -> bool {
        self.auto_mode
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "total_scans": self.total_scans,
            "total_threats_detected": self.total_threats_detected,
            "total_neutralized": self.total_neutralized,
            "auto_mode": self.auto_mode,
            "sites_scored": self.site_scores.len(),
        })
    }

    fn build_signature_database() -> Vec<PatternSignature> {
        vec![
            PatternSignature {
                pattern: HostilePattern::Paywall,
                html_indicators: vec!["paywall", "subscribe to read", "premium content", "unlock this article", "members only"],
                css_selectors: vec![".paywall", "#paywall", ".pw-overlay", "[data-paywall]"],
                class_patterns: vec!["class=\"paywall", "class=\"pw-", "id=\"paywall", "data-paywall"],
                neutralize_css: ".paywall, [data-paywall], .pw-overlay, .subscribe-wall { display: none !important; } body { overflow: auto !important; }",
                neutralize_js: "document.querySelectorAll('[data-paywall],.paywall,.pw-overlay').forEach(e => e.remove());",
            },
            PatternSignature {
                pattern: HostilePattern::DarkPattern,
                html_indicators: vec!["are you sure", "don't miss out", "last chance", "limited time", "act now", "hurry"],
                css_selectors: vec![".urgency", ".countdown", ".fomo"],
                class_patterns: vec!["class=\"urgency", "class=\"fomo", "class=\"countdown"],
                neutralize_css: ".urgency, .fomo, .countdown-timer, .scarcity-badge { display: none !important; }",
                neutralize_js: "document.querySelectorAll('.urgency,.fomo,.countdown-timer').forEach(e => e.remove());",
            },
            PatternSignature {
                pattern: HostilePattern::LoginWall,
                html_indicators: vec!["sign in to continue", "create an account", "log in to view", "register to access"],
                css_selectors: vec![".login-wall", ".auth-modal", "#login-overlay"],
                class_patterns: vec!["class=\"login-wall", "class=\"auth-modal", "id=\"login-overlay"],
                neutralize_css: ".login-wall, .auth-modal, #login-overlay, .login-gate { display: none !important; } body { overflow: auto !important; }",
                neutralize_js: "document.querySelectorAll('.login-wall,.auth-modal,#login-overlay').forEach(e => e.remove());",
            },
            PatternSignature {
                pattern: HostilePattern::GdprBanner,
                html_indicators: vec!["cookie consent", "we use cookies", "accept cookies", "cookie policy", "gdpr"],
                css_selectors: vec![".cookie-banner", "#cookie-consent", ".gdpr-banner"],
                class_patterns: vec!["class=\"cookie", "id=\"cookie", "class=\"gdpr", "id=\"consent"],
                neutralize_css: ".cookie-banner, #cookie-consent, .gdpr-banner, .cc-banner, #CybotCookiebotDialog { display: none !important; }",
                neutralize_js: "document.querySelectorAll('.cookie-banner,#cookie-consent,.gdpr-banner,.cc-banner').forEach(e => e.remove());",
            },
            PatternSignature {
                pattern: HostilePattern::NotificationSpam,
                html_indicators: vec!["enable notifications", "allow notifications", "push notifications", "bell icon"],
                css_selectors: vec![".notification-prompt", ".push-prompt"],
                class_patterns: vec!["class=\"notification-prompt", "class=\"push-"],
                neutralize_css: ".notification-prompt, .push-prompt, .web-push-modal { display: none !important; }",
                neutralize_js: "window.Notification = undefined; document.querySelectorAll('.notification-prompt,.push-prompt').forEach(e => e.remove());",
            },
            PatternSignature {
                pattern: HostilePattern::SeoGarbage,
                html_indicators: vec!["related articles", "you might also like", "recommended for you", "trending now", "popular posts"],
                css_selectors: vec![".related-articles", ".recommended", ".trending"],
                class_patterns: vec!["class=\"related", "class=\"recommended", "class=\"trending"],
                neutralize_css: ".related-articles, .recommended-section, .trending-sidebar { opacity: 0.3; }",
                neutralize_js: "",
            },
            PatternSignature {
                pattern: HostilePattern::AdOverload,
                html_indicators: vec!["advertisement", "sponsored content", "ad-container", "google_ads", "doubleclick"],
                css_selectors: vec![".ad-container", ".sponsored", "[data-ad]"],
                class_patterns: vec!["class=\"ad-", "class=\"ads", "id=\"ad-", "data-ad="],
                neutralize_css: ".ad-container, [data-ad], .sponsored, .ad-wrapper, .ad-slot, iframe[src*='doubleclick'] { display: none !important; }",
                neutralize_js: "document.querySelectorAll('.ad-container,[data-ad],.sponsored,.ad-wrapper').forEach(e => e.remove());",
            },
            PatternSignature {
                pattern: HostilePattern::CookieWall,
                html_indicators: vec!["accept all cookies", "accept & continue", "cookie wall", "consent required"],
                css_selectors: vec![".cookie-wall", "#cookie-wall"],
                class_patterns: vec!["class=\"cookie-wall", "id=\"cookie-wall", "class=\"consent-wall"],
                neutralize_css: ".cookie-wall, .consent-wall, .consent-overlay { display: none !important; } body { overflow: auto !important; }",
                neutralize_js: "document.querySelectorAll('.cookie-wall,.consent-wall,.consent-overlay').forEach(e => e.remove());",
            },
            PatternSignature {
                pattern: HostilePattern::AutoplayVideo,
                html_indicators: vec!["autoplay", "auto-play", "video-player"],
                css_selectors: vec!["video[autoplay]", ".auto-play-video"],
                class_patterns: vec!["autoplay=\"\"", "autoplay=\"true\"", "autoplay=\"autoplay\""],
                neutralize_css: "video[autoplay] { display: none !important; }",
                neutralize_js: "document.querySelectorAll('video').forEach(v => { v.pause(); v.autoplay = false; });",
            },
            PatternSignature {
                pattern: HostilePattern::PopupHell,
                html_indicators: vec!["popup", "modal-overlay", "exit-intent", "interstitial"],
                css_selectors: vec![".popup-overlay", ".modal-backdrop", ".exit-intent"],
                class_patterns: vec!["class=\"popup", "class=\"modal-overlay", "class=\"exit-intent", "class=\"interstitial"],
                neutralize_css: ".popup-overlay, .modal-backdrop, .exit-intent-popup, .interstitial-ad { display: none !important; } body { overflow: auto !important; }",
                neutralize_js: "document.querySelectorAll('.popup-overlay,.exit-intent-popup,.interstitial-ad').forEach(e => e.remove());",
            },
            PatternSignature {
                pattern: HostilePattern::ForcedNewsletter,
                html_indicators: vec!["subscribe to our newsletter", "join our mailing list", "get updates", "email signup"],
                css_selectors: vec![".newsletter-popup", ".email-signup", ".subscribe-modal"],
                class_patterns: vec!["class=\"newsletter", "class=\"email-signup", "class=\"subscribe-modal"],
                neutralize_css: ".newsletter-popup, .email-signup-modal, .subscribe-modal, .newsletter-overlay { display: none !important; }",
                neutralize_js: "document.querySelectorAll('.newsletter-popup,.subscribe-modal,.newsletter-overlay').forEach(e => e.remove());",
            },
        ]
    }
}
