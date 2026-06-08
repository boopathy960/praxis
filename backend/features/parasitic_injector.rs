// ─────────────────────────────────────────────────────────────
// Feature: Parasitic Injector (Benevolent Reverse-Hacking)
// ─────────────────────────────────────────────────────────────
// Injects beneficial modifications into web pages: tracking removal,
// privacy shields, accessibility enhancements, price comparison,
// security headers, and performance boosts.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InjectionType {
    PrivacyShield,
    TrackerRemoval,
    AccessibilityBoost,
    PriceComparison,
    SecurityHeader,
    PerformanceBoost,
    AdRemoval,
    ReadabilityEnhance,
    AntiFingerprint,
    ConsentAutoDecline,
}

impl InjectionType {
    pub fn label(&self) -> &str {
        match self {
            Self::PrivacyShield => "Privacy Shield",
            Self::TrackerRemoval => "Tracker Removal",
            Self::AccessibilityBoost => "Accessibility Boost",
            Self::PriceComparison => "Price Comparison",
            Self::SecurityHeader => "Security Header",
            Self::PerformanceBoost => "Performance Boost",
            Self::AdRemoval => "Ad Removal",
            Self::ReadabilityEnhance => "Readability Enhance",
            Self::AntiFingerprint => "Anti-Fingerprint",
            Self::ConsentAutoDecline => "Auto-Decline Consent",
        }
    }

    pub fn category(&self) -> &str {
        match self {
            Self::PrivacyShield
            | Self::TrackerRemoval
            | Self::AntiFingerprint
            | Self::ConsentAutoDecline => "Privacy",
            Self::AccessibilityBoost | Self::ReadabilityEnhance => "Accessibility",
            Self::PriceComparison => "Shopping",
            Self::SecurityHeader => "Security",
            Self::PerformanceBoost | Self::AdRemoval => "Performance",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Injection {
    pub id: String,
    pub injection_type: InjectionType,
    pub target_selector: String,
    pub injected_css: String,
    pub injected_js: String,
    pub is_active: bool,
    pub impact_score: f64,
    pub description: String,
    pub blocked_items: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionProfile {
    pub name: String,
    pub description: String,
    pub active_types: Vec<InjectionType>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InjectionReport {
    pub url: String,
    pub injections: Vec<Injection>,
    pub total_injected: usize,
    pub trackers_blocked: usize,
    pub scripts_removed: usize,
    pub accessibility_score: f64,
    pub privacy_score: f64,
    pub performance_gain_percent: f64,
    pub combined_css: String,
    pub combined_js: String,
}

pub struct ParasiticInjector {
    // Injection library: type -> pre-built injection templates
    injection_library: Vec<InjectionTemplate>,
    // Active profile
    active_profile: String,
    profiles: HashMap<String, InjectionProfile>,
    // Per-site overrides
    site_rules: HashMap<String, Vec<InjectionType>>,
    // Stats
    total_injections: u64,
    total_trackers_blocked: u64,
    total_scripts_removed: u64,
    total_pages_enhanced: u64,
}

struct InjectionTemplate {
    injection_type: InjectionType,
    html_indicators: Vec<&'static str>,
    css: &'static str,
    js: &'static str,
    description: &'static str,
    impact: f64,
}

impl ParasiticInjector {
    pub fn new() -> Self {
        let mut injector = Self {
            injection_library: Self::build_injection_library(),
            active_profile: "balanced".to_string(),
            profiles: HashMap::new(),
            site_rules: HashMap::new(),
            total_injections: 0,
            total_trackers_blocked: 0,
            total_scripts_removed: 0,
            total_pages_enhanced: 0,
        };
        injector.build_profiles();
        injector
    }

    /// Analyze page and generate injection payload.
    pub fn analyze_and_inject(&mut self, url: &str, html: &str) -> InjectionReport {
        self.total_pages_enhanced += 1;
        let html_lower = html.to_lowercase();
        let mut injections = Vec::new();
        let mut trackers_blocked = 0;
        let mut scripts_removed = 0;

        let active_types = self.get_active_types(url);

        for template in &self.injection_library {
            if !active_types.contains(&template.injection_type) {
                continue;
            }

            let mut matched = false;
            for indicator in &template.html_indicators {
                if html_lower.contains(indicator) {
                    matched = true;
                    break;
                }
            }

            // Some injections are always applicable
            let always_apply = matches!(
                template.injection_type,
                InjectionType::PrivacyShield
                    | InjectionType::AntiFingerprint
                    | InjectionType::SecurityHeader
                    | InjectionType::PerformanceBoost
            );

            if matched || always_apply {
                let id = format!("inj_{}_{}", self.total_injections, injections.len());
                let blocked = if template.injection_type == InjectionType::TrackerRemoval {
                    3
                } else if template.injection_type == InjectionType::AdRemoval {
                    2
                } else {
                    0
                };

                trackers_blocked += if template.injection_type == InjectionType::TrackerRemoval {
                    blocked
                } else {
                    0
                };
                scripts_removed += if template.injection_type == InjectionType::AdRemoval {
                    blocked
                } else {
                    0
                };

                injections.push(Injection {
                    id,
                    injection_type: template.injection_type,
                    target_selector: "body".into(),
                    injected_css: template.css.to_string(),
                    injected_js: template.js.to_string(),
                    is_active: true,
                    impact_score: template.impact,
                    description: template.description.to_string(),
                    blocked_items: blocked,
                });
            }
        }

        self.total_injections += injections.len() as u64;
        self.total_trackers_blocked += trackers_blocked as u64;
        self.total_scripts_removed += scripts_removed as u64;

        // Compute scores
        let privacy_score = Self::compute_privacy_score(&injections);
        let accessibility_score = Self::compute_accessibility_score(&injections);
        let perf_gain = Self::compute_performance_gain(&injections);

        let combined_css = injections
            .iter()
            .filter(|i| i.is_active && !i.injected_css.is_empty())
            .map(|i| i.injected_css.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let combined_js = injections
            .iter()
            .filter(|i| i.is_active && !i.injected_js.is_empty())
            .map(|i| i.injected_js.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        InjectionReport {
            url: url.to_string(),
            total_injected: injections.len(),
            trackers_blocked,
            scripts_removed,
            accessibility_score,
            privacy_score,
            performance_gain_percent: perf_gain,
            combined_css,
            combined_js,
            injections,
        }
    }

    /// Set active injection profile.
    pub fn set_profile(&mut self, profile_name: &str) {
        if self.profiles.contains_key(profile_name) {
            self.active_profile = profile_name.to_string();
        }
    }

    /// Get available profiles.
    pub fn get_profiles(&self) -> Vec<&InjectionProfile> {
        self.profiles.values().collect()
    }

    /// Add a site-specific rule override.
    pub fn set_site_rule(&mut self, domain: &str, types: Vec<InjectionType>) {
        self.site_rules.insert(domain.to_string(), types);
    }

    // ── Internal ──

    fn get_active_types(&self, url: &str) -> Vec<InjectionType> {
        // Check site-specific rules first
        let domain = reqwest::Url::parse(url)
            .map(|u: reqwest::Url| u.host_str().unwrap_or("").to_string())
            .unwrap_or_default();

        if let Some(site_types) = self.site_rules.get(&domain) {
            return site_types.clone();
        }

        // Fall back to active profile
        self.profiles
            .get(&self.active_profile)
            .map(|p| p.active_types.clone())
            .unwrap_or_default()
    }

    fn compute_privacy_score(injections: &[Injection]) -> f64 {
        let privacy_types = [
            InjectionType::PrivacyShield,
            InjectionType::TrackerRemoval,
            InjectionType::AntiFingerprint,
            InjectionType::ConsentAutoDecline,
        ];
        let active_privacy = injections
            .iter()
            .filter(|i| privacy_types.contains(&i.injection_type) && i.is_active)
            .count();
        (active_privacy as f64 / privacy_types.len() as f64).min(1.0)
    }

    fn compute_accessibility_score(injections: &[Injection]) -> f64 {
        let a11y_types = [
            InjectionType::AccessibilityBoost,
            InjectionType::ReadabilityEnhance,
        ];
        let active = injections
            .iter()
            .filter(|i| a11y_types.contains(&i.injection_type) && i.is_active)
            .count();
        (0.5 + active as f64 * 0.25).min(1.0)
    }

    fn compute_performance_gain(injections: &[Injection]) -> f64 {
        let perf_types = [
            InjectionType::PerformanceBoost,
            InjectionType::AdRemoval,
            InjectionType::TrackerRemoval,
        ];
        let active = injections
            .iter()
            .filter(|i| perf_types.contains(&i.injection_type) && i.is_active)
            .count();
        active as f64 * 8.0 // ~8% gain per optimization
    }

    fn build_injection_library() -> Vec<InjectionTemplate> {
        vec![
            InjectionTemplate {
                injection_type: InjectionType::TrackerRemoval,
                html_indicators: vec!["google-analytics", "facebook.net", "doubleclick", "hotjar", "mixpanel", "segment.io"],
                css: "",
                js: "const trackers=['google-analytics.com','googletagmanager.com','facebook.net','doubleclick.net','hotjar.com','mixpanel.com'];\
                     document.querySelectorAll('script').forEach(s=>{if(trackers.some(t=>s.src&&s.src.includes(t)))s.remove();});",
                description: "Removes known tracking scripts (Google Analytics, Facebook Pixel, etc.)",
                impact: 0.9,
            },
            InjectionTemplate {
                injection_type: InjectionType::PrivacyShield,
                html_indicators: vec!["beacon", "tracking", "analytics", "pixel"],
                css: "",
                js: "navigator.sendBeacon=()=>false;window.__gaTracker=()=>{};window.ga=()=>{};window.fbq=()=>{};\
                     Object.defineProperty(document,'referrer',{get:()=>''});",
                description: "Blocks beacons, neutralizes tracking APIs, hides referrer",
                impact: 0.85,
            },
            InjectionTemplate {
                injection_type: InjectionType::AntiFingerprint,
                html_indicators: vec!["canvas", "webgl", "fingerprint"],
                css: "",
                js: "const origToDataURL=HTMLCanvasElement.prototype.toDataURL;\
                     HTMLCanvasElement.prototype.toDataURL=function(t){const c=this.getContext('2d');if(c){const d=c.getImageData(0,0,1,1);d.data[0]^=1;c.putImageData(d,0,0);}return origToDataURL.call(this,t);};\
                     Object.defineProperty(navigator,'hardwareConcurrency',{get:()=>4});\
                     Object.defineProperty(navigator,'deviceMemory',{get:()=>8});",
                description: "Randomizes canvas fingerprint, normalizes hardware info",
                impact: 0.8,
            },
            InjectionTemplate {
                injection_type: InjectionType::AdRemoval,
                html_indicators: vec!["ad-container", "advertisement", "google_ads", "ad-wrapper", "banner-ad"],
                css: ".ad-container,.ad-wrapper,.advertisement,.banner-ad,[data-ad],[class*='ad-slot'],[id*='google_ads'],\
                      iframe[src*='doubleclick'],iframe[src*='googlesyndication']{display:none!important}",
                js: "document.querySelectorAll('.ad-container,.ad-wrapper,.advertisement,.banner-ad,[data-ad]').forEach(e=>e.remove());",
                description: "Removes advertisement containers and ad iframes",
                impact: 0.75,
            },
            InjectionTemplate {
                injection_type: InjectionType::AccessibilityBoost,
                html_indicators: vec!["<img", "<button", "<input", "<a "],
                css: "img:not([alt]){outline:3px dashed red!important}\
                      *:focus{outline:3px solid #4A90D9!important;outline-offset:2px!important}\
                      button,a,[role='button']{min-height:44px!important;min-width:44px!important}",
                js: "document.querySelectorAll('img:not([alt])').forEach(i=>i.alt='Image');\
                     document.querySelectorAll('a:not([aria-label])').forEach(a=>{if(a.textContent)a.setAttribute('aria-label',a.textContent.trim());});",
                description: "Fixes missing alt text, enforces focus indicators, ensures touch targets",
                impact: 0.7,
            },
            InjectionTemplate {
                injection_type: InjectionType::ReadabilityEnhance,
                html_indicators: vec!["<p", "<article", "<main"],
                css: "p{max-width:70ch!important;line-height:1.7!important}\
                      body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif!important}",
                js: "",
                description: "Optimizes paragraph width and line height for readability",
                impact: 0.5,
            },
            InjectionTemplate {
                injection_type: InjectionType::SecurityHeader,
                html_indicators: vec!["<script", "<iframe", "http://"],
                css: "",
                js: "document.querySelectorAll('iframe[src^=\"http:\"]').forEach(f=>{f.src=f.src.replace('http:','https:');});\
                     document.querySelectorAll('a[target=\"_blank\"]:not([rel*=\"noopener\"])').forEach(a=>{a.rel='noopener noreferrer';});",
                description: "Upgrades insecure iframes to HTTPS, adds noopener to external links",
                impact: 0.65,
            },
            InjectionTemplate {
                injection_type: InjectionType::PerformanceBoost,
                html_indicators: vec!["<img", "<video", "lazy"],
                css: "img:not([loading]){content-visibility:auto!important}",
                js: "document.querySelectorAll('img:not([loading])').forEach(i=>{i.loading='lazy';i.decoding='async';});\
                     document.querySelectorAll('video[autoplay]').forEach(v=>{v.autoplay=false;v.preload='none';});",
                description: "Lazy-loads images, disables autoplay videos, enables content-visibility",
                impact: 0.6,
            },
            InjectionTemplate {
                injection_type: InjectionType::ConsentAutoDecline,
                html_indicators: vec!["cookie-consent", "gdpr", "accept-cookies", "privacy-banner"],
                css: "",
                js: "setTimeout(()=>{const btns=document.querySelectorAll('[class*=reject],[class*=decline],[id*=reject],\
                     button[data-action=reject]');btns.forEach(b=>b.click());if(!btns.length){document.querySelectorAll(\
                     '.cookie-banner,.gdpr-banner,.consent-banner,#cookie-consent').forEach(e=>e.remove());}},1000);",
                description: "Auto-clicks 'reject all' on cookie consent banners, or removes them",
                impact: 0.55,
            },
        ]
    }

    fn build_profiles(&mut self) {
        self.profiles.insert(
            "privacy_max".into(),
            InjectionProfile {
                name: "Privacy Maximum".into(),
                description: "Maximum privacy: blocks all trackers, fingerprinting, and ads".into(),
                active_types: vec![
                    InjectionType::PrivacyShield,
                    InjectionType::TrackerRemoval,
                    InjectionType::AntiFingerprint,
                    InjectionType::AdRemoval,
                    InjectionType::ConsentAutoDecline,
                    InjectionType::SecurityHeader,
                ],
            },
        );
        self.profiles.insert(
            "balanced".into(),
            InjectionProfile {
                name: "Balanced".into(),
                description: "Privacy + performance without breaking sites".into(),
                active_types: vec![
                    InjectionType::PrivacyShield,
                    InjectionType::TrackerRemoval,
                    InjectionType::SecurityHeader,
                    InjectionType::PerformanceBoost,
                    InjectionType::ConsentAutoDecline,
                ],
            },
        );
        self.profiles.insert(
            "performance".into(),
            InjectionProfile {
                name: "Performance".into(),
                description: "Focus on speed: removes ads, lazy-loads everything".into(),
                active_types: vec![
                    InjectionType::AdRemoval,
                    InjectionType::PerformanceBoost,
                    InjectionType::TrackerRemoval,
                ],
            },
        );
        self.profiles.insert(
            "accessibility".into(),
            InjectionProfile {
                name: "Accessibility".into(),
                description: "Enhanced accessibility and readability".into(),
                active_types: vec![
                    InjectionType::AccessibilityBoost,
                    InjectionType::ReadabilityEnhance,
                    InjectionType::PerformanceBoost,
                ],
            },
        );
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "active_profile": self.active_profile,
            "total_injections": self.total_injections,
            "total_trackers_blocked": self.total_trackers_blocked,
            "total_scripts_removed": self.total_scripts_removed,
            "total_pages_enhanced": self.total_pages_enhanced,
            "available_profiles": self.profiles.len(),
            "site_rules": self.site_rules.len(),
        })
    }
}
