//! Metrics in the Prometheus text format (docs/operations.md, "Metrics").
//!
//! Off unless `METRICS_ADDR` is set, and then served on a listener of its
//! own, never on the public port: what the service is doing is not for
//! everyone who can reach the API.
//!
//! Written by hand rather than through a metrics library. There are a dozen
//! families, the text format is small and stable, and keeping the counts in
//! plain structs that the API state and the worker own, instead of a
//! process-wide recorder, lets every test read its own numbers. It also adds
//! nothing to the dependency tree that the weekly audit has to watch.
//!
//! Labels are kept to bounded sets: an HTTP request is counted under the
//! route's template (`/v1/exchanges/{id}`), never the path it was called
//! with, which would grow a series for every exchange.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::Router;
use axum::http::header::CONTENT_TYPE;
use axum::http::{Method, StatusCode};
use axum::routing::get;
use sqlx::PgPool;

use crate::auth::{SMS_FAILED, SMS_REFUSED, SMS_REFUSED_COUNTRY, SMS_REFUSED_PREFIX, SMS_SENT};
use crate::notifications::outbox::Delivered;
use crate::notifications::push::{PushDelivered, ReceiptsError, ReceiptsRead};

/// The media type of the text format, version 0.0.4.
pub const TEXT_FORMAT: &str = "text/plain; version=0.0.4; charset=utf-8";

/// Upper bounds, in seconds, of the request latency histogram's buckets.
/// From a millisecond, under the load check's fastest requests, to ten
/// seconds, past which a request has failed for any practical purpose.
pub const LATENCY_BUCKETS: [f64; 13] = [
    0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// The label for a request that matched no API route: the web app's pages
/// and assets, and anything unknown.
pub const UNMATCHED: &str = "unmatched";

// ---- The text format -------------------------------------------------------

/// What kind of metric a family is.
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Counter,
    Gauge,
    Histogram,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Counter => "counter",
            Self::Gauge => "gauge",
            Self::Histogram => "histogram",
        }
    }
}

/// A page of metrics being written.
#[derive(Default)]
pub struct Text {
    out: String,
}

impl Text {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a family: its help line and type. Every sample of the family
    /// follows it.
    pub fn family(&mut self, name: &str, kind: Kind, help: &str) {
        let help = help.replace('\\', "\\\\").replace('\n', "\\n");
        let _ = writeln!(self.out, "# HELP {name} {help}");
        let _ = writeln!(self.out, "# TYPE {name} {}", kind.as_str());
    }

    /// One sample.
    pub fn sample(&mut self, name: &str, labels: &[(&str, &str)], value: f64) {
        self.out.push_str(name);
        if !labels.is_empty() {
            self.out.push('{');
            for (index, (label, value)) in labels.iter().enumerate() {
                if index > 0 {
                    self.out.push(',');
                }
                let _ = write!(self.out, "{label}=\"{}\"", escape_label(value));
            }
            self.out.push('}');
        }
        let _ = writeln!(self.out, " {}", number(value));
    }

    /// A family with a single unlabeled sample.
    pub fn single(&mut self, name: &str, kind: Kind, help: &str, value: f64) {
        self.family(name, kind, help);
        self.sample(name, &[], value);
    }

    pub fn finish(self) -> String {
        self.out
    }
}

/// A label value, escaped as the format requires.
fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn number(value: f64) -> String {
    if value == f64::INFINITY {
        "+Inf".to_owned()
    } else if value == f64::NEG_INFINITY {
        "-Inf".to_owned()
    } else if value.is_nan() {
        "NaN".to_owned()
    } else {
        format!("{value}")
    }
}

// ---- HTTP requests ---------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct HttpSeries {
    route: String,
    method: &'static str,
    status: &'static str,
}

#[derive(Clone, Debug, Default)]
struct Histogram {
    /// Observations at or below each bound in [`LATENCY_BUCKETS`], each
    /// counted once, in the first bucket it fits; summed when written.
    buckets: [u64; LATENCY_BUCKETS.len()],
    count: u64,
    sum: f64,
}

impl Histogram {
    fn observe(&mut self, seconds: f64) {
        if let Some(bucket) = LATENCY_BUCKETS.iter().position(|bound| seconds <= *bound) {
            self.buckets[bucket] += 1;
        }
        self.count += 1;
        self.sum += seconds;
    }
}

/// Counts and latencies of the API's requests, by route template, method
/// and status class.
#[derive(Default)]
pub struct HttpMetrics {
    series: Mutex<BTreeMap<HttpSeries, Histogram>>,
}

/// A method as a label: the ones the API uses by name, anything else as
/// `OTHER`, so a client cannot invent series.
fn method_label(method: &Method) -> &'static str {
    match *method {
        Method::GET => "GET",
        Method::HEAD => "HEAD",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::PATCH => "PATCH",
        Method::DELETE => "DELETE",
        Method::OPTIONS => "OPTIONS",
        _ => "OTHER",
    }
}

fn status_class(status: StatusCode) -> &'static str {
    match status.as_u16() / 100 {
        1 => "1xx",
        2 => "2xx",
        3 => "3xx",
        4 => "4xx",
        _ => "5xx",
    }
}

impl HttpMetrics {
    /// Counts one request. `route` is the template of the route it matched,
    /// or none.
    pub fn observe(
        &self,
        method: &Method,
        route: Option<&str>,
        status: StatusCode,
        elapsed: Duration,
    ) {
        let series = HttpSeries {
            route: route.unwrap_or(UNMATCHED).to_owned(),
            method: method_label(method),
            status: status_class(status),
        };
        let mut all = self
            .series
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        all.entry(series)
            .or_default()
            .observe(elapsed.as_secs_f64());
    }

    pub fn render(&self, text: &mut Text) {
        let all = self
            .series
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();

        text.family(
            "yuppers_http_requests_total",
            Kind::Counter,
            "HTTP requests answered, by route template, method and status class.",
        );
        for (series, histogram) in &all {
            let labels = [
                ("route", series.route.as_str()),
                ("method", series.method),
                ("status", series.status),
            ];
            text.sample(
                "yuppers_http_requests_total",
                &labels,
                histogram.count as f64,
            );
        }

        let name = "yuppers_http_request_duration_seconds";
        text.family(
            name,
            Kind::Histogram,
            "Time from receiving a request to answering it, by route template, method and status class.",
        );
        for (series, histogram) in &all {
            let labels = [
                ("route", series.route.as_str()),
                ("method", series.method),
                ("status", series.status),
            ];
            let mut cumulative = 0;
            for (bound, count) in LATENCY_BUCKETS.iter().zip(histogram.buckets) {
                cumulative += count;
                let le = number(*bound);
                let labels = [labels.as_slice(), &[("le", le.as_str())]].concat();
                text.sample(&format!("{name}_bucket"), &labels, cumulative as f64);
            }
            let labels_inf = [labels.as_slice(), &[("le", "+Inf")]].concat();
            text.sample(
                &format!("{name}_bucket"),
                &labels_inf,
                histogram.count as f64,
            );
            text.sample(&format!("{name}_sum"), &labels, histogram.sum);
            text.sample(&format!("{name}_count"), &labels, histogram.count as f64);
        }
    }
}

// ---- The database ----------------------------------------------------------

/// The connection pool: its limit, how many connections are open and how
/// many of those are in use.
pub fn render_pool(text: &mut Text, pool: &PgPool) {
    let size = pool.size();
    let idle = u32::try_from(pool.num_idle()).unwrap_or(u32::MAX);
    text.single(
        "yuppers_db_pool_max",
        Kind::Gauge,
        "Connections the pool may open at most.",
        f64::from(pool.options().get_max_connections()),
    );
    text.single(
        "yuppers_db_pool_size",
        Kind::Gauge,
        "Connections the pool has open.",
        f64::from(size),
    );
    text.single(
        "yuppers_db_pool_in_use",
        Kind::Gauge,
        "Open connections in use by a request or a job.",
        f64::from(size.saturating_sub(idle)),
    );
}

/// The outbox as it stands in the database: messages waiting, messages
/// given up on, and how long the oldest waiting one has waited. Read at each
/// scrape, so it is right whichever process, or how many workers, did the
/// sending. Both the API and the worker report it, so it can still be seen
/// while the worker is down.
///
/// Also `yuppers_database_up`, 0 when the database could not be read.
pub async fn render_outbox(text: &mut Text, pool: &PgPool, max_attempts: i32) {
    // Only rows not yet completed, which the partial index
    // `outbox_pending_idx` holds; the sent ones are never read.
    let state: Result<(i64, i64, Option<f64>), sqlx::Error> = sqlx::query_as(
        "SELECT
             count(*) FILTER (WHERE attempts < $1),
             count(*) FILTER (WHERE attempts >= $1),
             EXTRACT(EPOCH FROM now() - min(created_at) FILTER (WHERE attempts < $1))::float8
         FROM outbox
         WHERE completed_at IS NULL",
    )
    .bind(max_attempts)
    .fetch_one(pool)
    .await;

    match state {
        Ok((pending, given_up, oldest)) => {
            text.single(
                "yuppers_database_up",
                Kind::Gauge,
                "1 if the database answered the last scrape's query, 0 if not.",
                1.0,
            );
            text.family(
                "yuppers_outbox_messages",
                Kind::Gauge,
                "Notifications not yet completed: pending (waiting, or between retries) and given_up (out of attempts, left for someone to look at).",
            );
            text.sample(
                "yuppers_outbox_messages",
                &[("state", "pending")],
                pending as f64,
            );
            text.sample(
                "yuppers_outbox_messages",
                &[("state", "given_up")],
                given_up as f64,
            );
            text.single(
                "yuppers_outbox_oldest_pending_age_seconds",
                Kind::Gauge,
                "How long the oldest pending notification has waited since it was queued; 0 when none is pending.",
                oldest.unwrap_or(0.0).max(0.0),
            );
        }
        Err(error) => {
            tracing::warn!(%error, "metrics could not read the outbox");
            text.single(
                "yuppers_database_up",
                Kind::Gauge,
                "1 if the database answered the last scrape's query, 0 if not.",
                0.0,
            );
        }
    }
}

// ---- Text messages ---------------------------------------------------------

/// The one-time codes sent by text message in the current hour, by the whole
/// service, against the hourly cap (`SMS_MAX_PER_HOUR`): read from the counts
/// the API keeps in the database, so they are right however many copies of
/// the API there are. Served by the API when SMS delivery is on.
pub async fn render_sms(text: &mut Text, pool: &PgPool, cap: i64) {
    let counts: Result<Vec<(String, i64)>, sqlx::Error> = sqlx::query_as(
        "SELECT scope, sum(count)::bigint FROM sign_in_limit
         WHERE scope IN ($1, $2, $3, $4, $5)
           AND window_start = date_trunc('hour', now(), 'UTC')
         GROUP BY scope",
    )
    .bind(SMS_SENT)
    .bind(SMS_REFUSED)
    .bind(SMS_FAILED)
    .bind(SMS_REFUSED_PREFIX)
    .bind(SMS_REFUSED_COUNTRY)
    .fetch_all(pool)
    .await;
    text.single(
        "yuppers_sms_codes_hourly_cap",
        Kind::Gauge,
        "Codes the service may send by text message per hour (SMS_MAX_PER_HOUR).",
        cap as f64,
    );
    let Ok(counts) = counts else {
        tracing::warn!("metrics could not read the text message counts");
        return;
    };
    let count = |scope: &str| {
        counts
            .iter()
            .find(|(found, _)| found == scope)
            .map_or(0, |(_, count)| *count)
    };
    let refusals = [
        ("hourly_cap", count(SMS_REFUSED)),
        ("prefix_cap", count(SMS_REFUSED_PREFIX)),
        ("country", count(SMS_REFUSED_COUNTRY)),
    ];
    let name = "yuppers_sms_codes_this_hour";
    text.family(
        name,
        Kind::Gauge,
        "One-time codes for phone numbers this hour (UTC), by the whole service: sent (taken by the SMS provider, each a message paid for), refused (not sent, for any reason in yuppers_sms_codes_refused_this_hour; the person was told), failed (the provider did not take it; not counted in sent).",
    );
    for (result, value) in [
        ("sent", count(SMS_SENT)),
        ("refused", refusals.iter().map(|(_, n)| n).sum()),
        ("failed", count(SMS_FAILED)),
    ] {
        text.sample(name, &[("result", result)], value as f64);
    }
    let name = "yuppers_sms_codes_refused_this_hour";
    text.family(
        name,
        Kind::Gauge,
        "One-time codes for phone numbers refused this hour (UTC), by the whole service, by reason: hourly_cap (SMS_MAX_PER_HOUR), prefix_cap (SMS_MAX_PER_PREFIX_PER_HOUR, numbers beginning alike), country (not in SMS_ALLOWED_COUNTRY_CODES).",
    );
    for (reason, value) in refusals {
        text.sample(name, &[("reason", reason)], value as f64);
    }
}

// ---- The worker ------------------------------------------------------------

/// What this worker process has done since it started.
#[derive(Default)]
pub struct WorkerMetrics {
    sent: AtomicU64,
    failed: AtomicU64,
    given_up: AtomicU64,
    dropped: AtomicU64,
    push_sent: AtomicU64,
    push_failed: AtomicU64,
    push_given_up: AtomicU64,
    push_dropped: AtomicU64,
    push_devices_removed: AtomicU64,
    receipt_checks_ok: AtomicU64,
    receipt_checks_failed: AtomicU64,
    timer_runs_ok: AtomicU64,
    timer_runs_failed: AtomicU64,
    timer_changes: AtomicU64,
    reminder_runs_ok: AtomicU64,
    reminder_runs_failed: AtomicU64,
    reminders_queued: AtomicU64,
    last_pass: AtomicU64,
}

fn add(counter: &AtomicU64, amount: usize) {
    counter.fetch_add(amount as u64, Ordering::Relaxed);
}

fn read(counter: &AtomicU64) -> f64 {
    counter.load(Ordering::Relaxed) as f64
}

impl WorkerMetrics {
    pub fn timers(&self, result: &Result<usize, sqlx::Error>) {
        match result {
            Ok(changed) => {
                add(&self.timer_runs_ok, 1);
                add(&self.timer_changes, *changed);
            }
            Err(_) => add(&self.timer_runs_failed, 1),
        }
    }

    pub fn reminders(&self, result: &Result<usize, sqlx::Error>) {
        match result {
            Ok(queued) => {
                add(&self.reminder_runs_ok, 1);
                add(&self.reminders_queued, *queued);
            }
            Err(_) => add(&self.reminder_runs_failed, 1),
        }
    }

    pub fn delivered(&self, delivered: &Delivered) {
        add(&self.sent, delivered.sent);
        add(&self.failed, delivered.failed);
        add(&self.given_up, delivered.given_up);
        add(&self.dropped, delivered.dropped);
    }

    pub fn pushed(&self, pushed: &PushDelivered) {
        add(&self.push_sent, pushed.rows.sent);
        add(&self.push_failed, pushed.rows.failed);
        add(&self.push_given_up, pushed.rows.given_up);
        add(&self.push_dropped, pushed.rows.dropped);
        add(&self.push_devices_removed, pushed.devices_removed);
    }

    pub fn receipts(&self, result: &Result<ReceiptsRead, ReceiptsError>) {
        match result {
            Ok(read) => {
                // A look with nothing old enough to ask about asks nothing.
                if read.asked {
                    add(&self.receipt_checks_ok, 1);
                }
                add(&self.push_devices_removed, read.devices_removed);
            }
            Err(_) => add(&self.receipt_checks_failed, 1),
        }
    }

    /// A pass over every job has finished, at `unix_seconds`.
    pub fn pass_finished(&self, unix_seconds: u64) {
        self.last_pass.store(unix_seconds, Ordering::Relaxed);
    }

    pub fn render(&self, text: &mut Text) {
        let name = "yuppers_outbox_deliveries_total";
        text.family(
            name,
            Kind::Counter,
            "Notification sends by this worker since it started: sent, failed (every failed try, given up or not), given_up (the last try failed), dropped (closed unsent).",
        );
        for (result, counter) in [
            ("sent", &self.sent),
            ("failed", &self.failed),
            ("given_up", &self.given_up),
            ("dropped", &self.dropped),
        ] {
            text.sample(name, &[("result", result)], read(counter));
        }

        let name = "yuppers_push_deliveries_total";
        text.family(
            name,
            Kind::Counter,
            "Push notifications handled by this worker since it started, one per person and notice: sent (taken by the push service for at least one device), failed (every failed try), given_up (the last try failed), dropped (closed unsent: no device left, account closed, reminder no longer true, or push off).",
        );
        for (result, counter) in [
            ("sent", &self.push_sent),
            ("failed", &self.push_failed),
            ("given_up", &self.push_given_up),
            ("dropped", &self.push_dropped),
        ] {
            text.sample(name, &[("result", result)], read(counter));
        }
        text.single(
            "yuppers_push_devices_removed_total",
            Kind::Counter,
            "Devices removed because the push service said their token is no longer registered, when sending or in a receipt.",
            read(&self.push_devices_removed),
        );
        let name = "yuppers_push_receipt_checks_total";
        text.family(
            name,
            Kind::Counter,
            "Requests for push receipts, by whether the push service answered.",
        );
        for (result, counter) in [
            ("ok", &self.receipt_checks_ok),
            ("error", &self.receipt_checks_failed),
        ] {
            text.sample(name, &[("result", result)], read(counter));
        }

        let name = "yuppers_worker_runs_total";
        text.family(
            name,
            Kind::Counter,
            "Passes of the worker's timers (expiries and closures) and reminders, by whether they finished or failed.",
        );
        for (job, result, counter) in [
            ("timers", "ok", &self.timer_runs_ok),
            ("timers", "error", &self.timer_runs_failed),
            ("reminders", "ok", &self.reminder_runs_ok),
            ("reminders", "error", &self.reminder_runs_failed),
        ] {
            text.sample(name, &[("job", job), ("result", result)], read(counter));
        }

        text.single(
            "yuppers_worker_timer_changes_total",
            Kind::Counter,
            "Exchanges the timers changed: revisions expired, close requests lapsed, inactivity prompts and closures.",
            read(&self.timer_changes),
        );
        text.single(
            "yuppers_worker_reminders_queued_total",
            Kind::Counter,
            "Reminders queued in the outbox.",
            read(&self.reminders_queued),
        );
        text.single(
            "yuppers_worker_last_pass_timestamp_seconds",
            Kind::Gauge,
            "When the worker last finished a pass over its jobs, in seconds since the Unix epoch; 0 before the first.",
            read(&self.last_pass),
        );
    }
}

// ---- Serving ---------------------------------------------------------------

/// Serves `GET /metrics` on `addr`, each page written by `render`, until the
/// process ends. Binds before returning, so an address that cannot be used
/// stops the process at start rather than failing quietly later.
pub async fn serve<F, Fut>(addr: SocketAddr, render: F) -> anyhow::Result<()>
where
    F: Fn() -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = String> + Send + 'static,
{
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|error| anyhow::anyhow!("METRICS_ADDR={addr} cannot be listened on: {error}"))?;
    let app = Router::new().route(
        "/metrics",
        get(move || {
            let render = render.clone();
            async move { ([(CONTENT_TYPE, TEXT_FORMAT)], render().await) }
        }),
    );
    tracing::info!(%addr, "metrics listening");
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::error!(%error, "metrics listener stopped");
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_written_in_the_text_format_with_labels_escaped() {
        let mut text = Text::new();
        text.family("demo_total", Kind::Counter, "A demo.\nSecond line.");
        text.sample("demo_total", &[], 3.0);
        text.sample(
            "demo_total",
            &[("a", "plain"), ("b", "quote \" back\\slash\nnewline")],
            0.25,
        );
        assert_eq!(
            text.finish(),
            "# HELP demo_total A demo.\\nSecond line.\n\
             # TYPE demo_total counter\n\
             demo_total 3\n\
             demo_total{a=\"plain\",b=\"quote \\\" back\\\\slash\\nnewline\"} 0.25\n"
        );
        assert_eq!(number(f64::INFINITY), "+Inf");
        assert_eq!(number(1e-3), "0.001");
    }

    #[test]
    fn a_request_is_counted_under_its_route_method_and_status_class() {
        let metrics = HttpMetrics::default();
        let route = Some("/v1/exchanges/{id}");
        metrics.observe(
            &Method::GET,
            route,
            StatusCode::OK,
            Duration::from_millis(3),
        );
        metrics.observe(
            &Method::GET,
            route,
            StatusCode::NO_CONTENT,
            Duration::from_millis(30),
        );
        metrics.observe(
            &Method::GET,
            route,
            StatusCode::NOT_FOUND,
            Duration::from_millis(1),
        );
        metrics.observe(
            &Method::from_bytes(b"BREW").unwrap(),
            None,
            StatusCode::INTERNAL_SERVER_ERROR,
            Duration::from_secs(20),
        );
        let mut text = Text::new();
        metrics.render(&mut text);
        let page = text.finish();

        let ok = r#"route="/v1/exchanges/{id}",method="GET",status="2xx""#;
        assert!(
            page.contains(&format!("yuppers_http_requests_total{{{ok}}} 2\n")),
            "{page}"
        );
        assert!(page.contains(
            r#"yuppers_http_requests_total{route="/v1/exchanges/{id}",method="GET",status="4xx"} 1"#
        ));
        assert!(page.contains(
            r#"yuppers_http_requests_total{route="unmatched",method="OTHER",status="5xx"} 1"#
        ));
        // Cumulative buckets: 3 ms is under 5 ms, 30 ms only from 50 ms.
        for (le, count) in [
            ("0.0025", 0),
            ("0.005", 1),
            ("0.025", 1),
            ("0.05", 2),
            ("+Inf", 2),
        ] {
            let line = format!(
                "yuppers_http_request_duration_seconds_bucket{{{ok},le=\"{le}\"}} {count}\n"
            );
            assert!(page.contains(&line), "{line} in {page}");
        }
        assert!(page.contains(&format!(
            "yuppers_http_request_duration_seconds_count{{{ok}}} 2\n"
        )));
        let sum_prefix = format!("yuppers_http_request_duration_seconds_sum{{{ok}}} ");
        let sum: f64 = page
            .lines()
            .find_map(|line| line.strip_prefix(&sum_prefix))
            .expect("a sum")
            .parse()
            .unwrap();
        assert!((sum - 0.033).abs() < 1e-9, "{sum}");
        // Slower than every bound: only in +Inf.
        assert!(page.contains(r#"yuppers_http_request_duration_seconds_bucket{route="unmatched",method="OTHER",status="5xx",le="10"} 0"#));
        assert!(page.contains(r#"yuppers_http_request_duration_seconds_bucket{route="unmatched",method="OTHER",status="5xx",le="+Inf"} 1"#));
        assert_eq!(page.matches("# TYPE").count(), 2);
    }

    #[test]
    fn the_worker_counts_its_push_notifications_apart_from_its_emails() {
        let metrics = WorkerMetrics::default();
        metrics.pushed(&PushDelivered {
            rows: Delivered {
                sent: 3,
                failed: 1,
                dropped: 2,
                ..Delivered::default()
            },
            devices_removed: 1,
        });
        metrics.receipts(&Ok(ReceiptsRead {
            asked: true,
            settled: 4,
            devices_removed: 2,
        }));
        metrics.receipts(&Ok(ReceiptsRead::default()));
        metrics.receipts(&Err(ReceiptsError::Service(anyhow::anyhow!("down"))));
        let mut text = Text::new();
        metrics.render(&mut text);
        let page = text.finish();
        for line in [
            r#"yuppers_push_deliveries_total{result="sent"} 3"#,
            r#"yuppers_push_deliveries_total{result="failed"} 1"#,
            r#"yuppers_push_deliveries_total{result="given_up"} 0"#,
            r#"yuppers_push_deliveries_total{result="dropped"} 2"#,
            "yuppers_push_devices_removed_total 3",
            r#"yuppers_push_receipt_checks_total{result="ok"} 1"#,
            r#"yuppers_push_receipt_checks_total{result="error"} 1"#,
            r#"yuppers_outbox_deliveries_total{result="sent"} 0"#,
        ] {
            assert!(page.contains(line), "{line} in\n{page}");
        }
    }

    #[test]
    fn the_worker_counts_what_it_did() {
        let metrics = WorkerMetrics::default();
        metrics.timers(&Ok(2));
        metrics.timers(&Err(sqlx::Error::PoolTimedOut));
        metrics.reminders(&Ok(5));
        metrics.delivered(&Delivered {
            sent: 4,
            failed: 2,
            given_up: 1,
            dropped: 1,
            cut_short: false,
        });
        metrics.pass_finished(1_790_000_000);
        let mut text = Text::new();
        metrics.render(&mut text);
        let page = text.finish();
        for line in [
            r#"yuppers_outbox_deliveries_total{result="sent"} 4"#,
            r#"yuppers_outbox_deliveries_total{result="failed"} 2"#,
            r#"yuppers_outbox_deliveries_total{result="given_up"} 1"#,
            r#"yuppers_outbox_deliveries_total{result="dropped"} 1"#,
            r#"yuppers_worker_runs_total{job="timers",result="ok"} 1"#,
            r#"yuppers_worker_runs_total{job="timers",result="error"} 1"#,
            r#"yuppers_worker_runs_total{job="reminders",result="ok"} 1"#,
            r#"yuppers_worker_runs_total{job="reminders",result="error"} 0"#,
            "yuppers_worker_timer_changes_total 2",
            "yuppers_worker_reminders_queued_total 5",
            "yuppers_worker_last_pass_timestamp_seconds 1790000000",
        ] {
            assert!(page.contains(&format!("{line}\n")), "{line} in {page}");
        }
    }
}
