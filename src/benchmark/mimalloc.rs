//! mimalloc's end-of-run heap statistics, when the Factorio process prints them
//! (`heap stats: …`), for the report's "Memory (mimalloc)" section. Ported from B.E.L.T. 4.x,
//! including its pattern, so the same output yields the same report.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

/// How many lines after `heap stats:` the statistics span (the pattern reads up to `peak rss:`).
pub const STATS_LINES: usize = 32;

static STATS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"heap\sstats:\s*peak\s*total\s*current\s*block\s*total#\s*reserved:\s*(?P<reserved_peak>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<reserved_total>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<reserved_current>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*committed:\s*(?P<committed_peak>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<committed_total>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<committed_current>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*reset:\s*(?:\d+)\s*purged:\s*(?:\d+)\s*touched:\s*(?P<touched_peak>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<touched_total>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<touched_current>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<touched_status>(?:[[:alpha:]]+[[:blank:]]?)*)\s*pages:\s*(?P<pages_peak>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<pages_total>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<pages_current>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<pages_status>(?:[[:alpha:]]+[[:blank:]]?)*)\s*-abandoned:\s*(?P<abandoned_peak>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<abandoned_total>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<abandoned_current>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<abandoned_status>(?:[[:alpha:]]+[[:blank:]]?)*).*\n.*\n.*\n.*\n.*\n.*\n.*\n.*\n.*\n\s*mmaps:\s*(?P<mmaps>\d+)\s*commits:\s*(?P<commits>\d+)\s*resets:\s*(?P<resets>\d+)\s*purges:\s*(?P<purges>\d+).*\n.*\s*threads:\s*(?P<threads_peak>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<threads_total>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<threads_current>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<threads_status>(?:[[:alpha:]]+[[:blank:]]?)*)\n.*\n.*\n.*\n.*peak rss:\s(?P<rss_peak>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?).*",
    )
    .expect("the mimalloc pattern compiles")
});

/// One run's heap statistics, as the report shows them (sizes keep mimalloc's own units).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MimallocStats {
    pub committed_peak: String,
    pub peak_rss: String,
    pub reserved_peak: String,
    pub committed_current: String,
    pub reserved_current: String,
    pub pages_current: String,
    pub pages_status: String,
    pub abandoned_current: String,
    pub abandoned_status: String,
    pub threads_peak: u32,
    pub threads_total: u32,
    pub mmaps: String,
    pub purges: String,
    pub resets: String,
    /// Peak RSS over committed peak, as a percentage with one decimal (`12.5%`).
    pub commit_efficiency: String,
    /// Threads total minus threads peak.
    pub thread_churn: u32,
}

/// The statistics in `text` (Factorio's output from a `heap stats:` line on), or `None` when it
/// doesn't hold them in mimalloc's layout, or uses a size unit this parser doesn't know.
pub fn parse(text: &str) -> Option<MimallocStats> {
    let captures = STATS.captures(text)?;
    let text_of = |name: &str| {
        captures
            .name(name)
            .map(|found| found.as_str().trim().to_string())
    };
    let count = |name: &str| text_of(name)?.parse::<u32>().ok();
    let committed_bytes = bytes(&text_of("committed_peak")?)?;
    let rss_bytes = bytes(&text_of("rss_peak")?)?;
    let threads_peak = count("threads_peak")?;
    let threads_total = count("threads_total")?;
    Some(MimallocStats {
        committed_peak: text_of("committed_peak")?,
        peak_rss: text_of("rss_peak")?,
        reserved_peak: text_of("reserved_peak")?,
        committed_current: text_of("committed_current")?,
        reserved_current: text_of("reserved_current")?,
        pages_current: text_of("pages_current")?,
        pages_status: text_of("pages_status")?,
        abandoned_current: text_of("abandoned_current")?,
        abandoned_status: text_of("abandoned_status")?,
        threads_peak,
        threads_total,
        mmaps: text_of("mmaps")?,
        purges: text_of("purges")?,
        resets: text_of("resets")?,
        commit_efficiency: format!("{:.1}%", rss_bytes as f64 / committed_bytes as f64 * 100.0),
        thread_churn: threads_total.saturating_sub(threads_peak),
    })
}

/// `3.9 GiB` in bytes; a bare number is bytes. `Ki`, `Mi`, … are mimalloc's counts of 8 bytes.
fn bytes(size: &str) -> Option<u64> {
    let mut parts = size.split_whitespace();
    let value: f64 = parts.next()?.parse().ok()?;
    let multiplier: f64 = match parts.next() {
        None => 1.0,
        Some("KiB" | "KB") => 1024.0,
        Some("Ki") => 1024.0 * 8.0,
        Some("MiB" | "MB") => 1024.0 * 1024.0,
        Some("Mi") => 1024.0 * 1024.0 * 8.0,
        Some("GiB" | "GB") => 1024.0 * 1024.0 * 1024.0,
        Some("Gi") => 1024.0 * 1024.0 * 1024.0 * 8.0,
        Some("TiB" | "TB") => 1024.0_f64.powi(4),
        Some("Ti") => 1024.0_f64.powi(4) * 8.0,
        Some(_) => return None,
    };
    Some((value * multiplier) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// mimalloc's statistics layout, as its stats printer writes them at exit.
    const SAMPLE: &str = include_str!("mimalloc_sample.txt");

    #[test]
    fn reads_mimalloc_heap_stats_with_its_derived_values() {
        let stats = parse(SAMPLE).unwrap();

        let read = (
            stats.committed_peak.as_str(),
            stats.peak_rss.as_str(),
            stats.commit_efficiency.as_str(),
            stats.thread_churn,
        );
        assert_eq!(read, ("800.0 MiB", "400.0 MiB", "50.0%", 5));
    }

    #[test]
    fn output_without_heap_stats_has_none() {
        assert_eq!(parse("Performed 10 updates in 1.0 ms\n"), None);
    }
}
