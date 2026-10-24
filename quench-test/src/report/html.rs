//! The HTML page for a [`Report`]: one self-contained file with its own styles and a few lines of script for
//! filtering (the page reads fine without them).

use super::{Case, Report, Status, Suite, Summary};
use std::fmt::Write as _;

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

fn duration(ms: u64) -> String {
    if ms >= 60_000 {
        format!("{}m {:02}s", ms / 60_000, (ms / 1000) % 60)
    } else if ms >= 1000 {
        format!("{:.2}s", ms as f64 / 1000.0)
    } else {
        format!("{ms}ms")
    }
}

fn timestamp(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        rest / 3600,
        (rest / 60) % 60,
        rest % 60
    )
}

fn badge(status: Status) -> String {
    format!("<span class=\"badge {0}\">{0}</span>", status.as_str())
}

fn bar(summary: Summary) -> String {
    let total = summary.total().max(1) as f64;
    let width = |n: usize| n as f64 / total * 100.0;
    format!(
        "<div class=\"bar\" role=\"img\" aria-label=\"{} passed, {} failed, {} skipped\"><i class=\"passed\" style=\"width:{:.2}%\"></i><i class=\"failed\" style=\"width:{:.2}%\"></i><i class=\"skipped\" style=\"width:{:.2}%\"></i></div>",
        summary.passed,
        summary.failed,
        summary.skipped,
        width(summary.passed),
        width(summary.failed),
        width(summary.skipped)
    )
}

fn counts(summary: Summary) -> String {
    let mut parts = vec![format!("{} passed", summary.passed)];
    if summary.failed > 0 {
        parts.push(format!("{} failed", summary.failed));
    }
    if summary.skipped > 0 {
        parts.push(format!("{} skipped", summary.skipped));
    }
    parts.join(", ")
}

fn case_row(case: &Case, out: &mut String) {
    let status = case.status.as_str();
    let _ = write!(
        out,
        "<li class=\"case {status}\" data-status=\"{status}\" data-text=\"{}\"><details{}><summary>{} <span class=\"name\">{}</span>",
        escape(&format!("{} {}", case.name, case.tags.join(" ")).to_lowercase()),
        if case.status == Status::Failed {
            " open"
        } else {
            ""
        },
        badge(case.status),
        escape(&case.name)
    );
    for tag in &case.tags {
        let _ = write!(out, " <span class=\"tag\">{}</span>", escape(tag));
    }
    let _ = write!(
        out,
        " <span class=\"time\">{}</span></summary>",
        duration(case.duration_ms)
    );
    if let Some(location) = &case.location {
        let _ = write!(out, "<p class=\"location\">{}</p>", escape(location));
    }
    if !case.steps.is_empty() {
        out.push_str("<ol class=\"steps\">");
        for step in &case.steps {
            let _ = write!(
                out,
                "<li class=\"step {0}\"><span class=\"keyword\">{1}</span> {2} <span class=\"time\">{3}</span>",
                step.status.as_str(),
                escape(&step.keyword),
                escape(&step.text),
                duration(step.duration_ms)
            );
            if let Some(message) = &step.message {
                let _ = write!(out, "<pre>{}</pre>", escape(message));
            }
            out.push_str("</li>");
        }
        out.push_str("</ol>");
    }
    if let Some(message) = &case.message {
        let _ = write!(out, "<pre class=\"message\">{}</pre>", escape(message));
    }
    out.push_str("</details></li>");
}

fn suite_section(suite: &Suite, out: &mut String) {
    let summary = suite.summary();
    let open = summary.failed > 0;
    let _ = write!(
        out,
        "<section class=\"suite\" data-failed=\"{}\"><details{}><summary><span class=\"suite-name\">{}</span> <span class=\"kind\">{}</span> <span class=\"counts\">{}</span> <span class=\"time\">{}</span></summary>{}<ul class=\"cases\">",
        summary.failed,
        if open { " open" } else { "" },
        escape(&suite.name),
        escape(&suite.kind),
        counts(summary),
        duration(suite.duration_ms()),
        bar(summary)
    );
    // Failures first, then the rest in name order.
    let mut cases: Vec<&Case> = suite.cases.iter().collect();
    cases.sort_by(|a, b| {
        (a.status != Status::Failed, &a.name).cmp(&(b.status != Status::Failed, &b.name))
    });
    for case in cases {
        case_row(case, out);
    }
    out.push_str("</ul></details></section>");
}

const STYLE: &str = r"
:root{color-scheme:light dark;--bg:#fafafa;--fg:#1c1c1e;--muted:#6b6b72;--card:#fff;--line:#e3e3e8;--pass:#1a7f37;--fail:#cf222e;--skip:#8b8b93;--pass-bg:#dafbe1;--fail-bg:#ffebe9;--skip-bg:#eeeef0}
@media (prefers-color-scheme:dark){:root:not([data-theme=light]){--bg:#111114;--fg:#ececf0;--muted:#9b9ba5;--card:#1b1b20;--line:#303038;--pass:#4ac26b;--fail:#ff7b72;--skip:#9b9ba5;--pass-bg:#12261a;--fail-bg:#2d1517;--skip-bg:#26262c}}
:root[data-theme=dark]{--bg:#111114;--fg:#ececf0;--muted:#9b9ba5;--card:#1b1b20;--line:#303038;--pass:#4ac26b;--fail:#ff7b72;--skip:#9b9ba5;--pass-bg:#12261a;--fail-bg:#2d1517;--skip-bg:#26262c}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,-apple-system,Segoe UI,Roboto,sans-serif}
main{max-width:1100px;margin:0 auto;padding:24px 16px 64px}
h1{font-size:1.5rem;margin:0 0 4px}.sub{color:var(--muted);margin:0 0 20px}
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(130px,1fr));gap:12px;margin-bottom:16px}
.card{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:12px 14px}.card b{display:block;font-size:1.6rem;line-height:1.2}.card span{color:var(--muted);font-size:.85rem}
.card.failed b{color:var(--fail)}.card.passed b{color:var(--pass)}
.bar{display:flex;height:8px;border-radius:4px;overflow:hidden;background:var(--skip-bg);margin:10px 0}.bar i{display:block;height:100%}.bar .passed{background:var(--pass)}.bar .failed{background:var(--fail)}.bar .skipped{background:var(--skip)}
.toolbar{display:flex;flex-wrap:wrap;gap:8px;margin:16px 0}.toolbar input{flex:1 1 220px;padding:8px 10px;border:1px solid var(--line);border-radius:8px;background:var(--card);color:var(--fg);font:inherit}
.toolbar button{padding:8px 12px;border:1px solid var(--line);border-radius:8px;background:var(--card);color:var(--fg);font:inherit;cursor:pointer}.toolbar button[aria-pressed=true]{background:var(--fg);color:var(--bg)}
.suite{background:var(--card);border:1px solid var(--line);border-radius:10px;margin:10px 0;padding:0 14px}.suite>details>summary{cursor:pointer;padding:12px 0;list-style:none}
.suite-name{font-weight:600}.kind{font-size:.75rem;text-transform:uppercase;letter-spacing:.04em;color:var(--muted);border:1px solid var(--line);border-radius:4px;padding:0 5px;margin:0 6px}.counts,.time{color:var(--muted);font-size:.85rem}.time{float:right}
ul.cases{list-style:none;margin:0;padding:0 0 10px}.case{border-top:1px solid var(--line)}.case>details>summary{cursor:pointer;padding:7px 0;list-style:none}.name{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:.85rem;word-break:break-all}
.badge{display:inline-block;min-width:62px;text-align:center;font-size:.72rem;font-weight:600;text-transform:uppercase;border-radius:4px;padding:1px 6px}.badge.passed{background:var(--pass-bg);color:var(--pass)}.badge.failed{background:var(--fail-bg);color:var(--fail)}.badge.skipped{background:var(--skip-bg);color:var(--skip)}
.tag{font-size:.75rem;color:var(--muted);border:1px solid var(--line);border-radius:10px;padding:0 6px}
pre{background:var(--bg);border:1px solid var(--line);border-radius:6px;padding:10px;overflow:auto;white-space:pre-wrap;margin:6px 0 10px;font-size:.82rem}.case.failed pre.message{border-color:var(--fail)}
ol.steps{list-style:none;margin:4px 0 8px;padding-left:12px;border-left:2px solid var(--line)}.step{padding:2px 0}.step.failed>.keyword{color:var(--fail)}.step.skipped{color:var(--skip)}.keyword{font-weight:600}
.location{color:var(--muted);font-size:.8rem;margin:0 0 4px}.hidden{display:none!important}
";

const SCRIPT: &str = r"
(function(){
  var input=document.getElementById('q'),buttons=document.querySelectorAll('[data-filter]'),state='all';
  function apply(){
    var q=(input.value||'').toLowerCase();
    document.querySelectorAll('.case').forEach(function(li){
      var ok=(state==='all'||li.dataset.status===state)&&(!q||li.dataset.text.indexOf(q)>=0);
      li.classList.toggle('hidden',!ok);
    });
    document.querySelectorAll('.suite').forEach(function(s){
      var any=s.querySelector('.case:not(.hidden)');
      s.classList.toggle('hidden',!any);
    });
  }
  buttons.forEach(function(b){b.addEventListener('click',function(){
    state=b.dataset.filter;
    buttons.forEach(function(o){o.setAttribute('aria-pressed',o===b?'true':'false')});
    apply();
  })});
  input.addEventListener('input',apply);
})();
";

/// The report as a complete HTML document.
#[must_use]
pub fn html(report: &Report) -> String {
    let summary = report.summary();
    let verdict = if summary.failed > 0 {
        "failed"
    } else {
        "passed"
    };
    let mut out = String::new();
    let _ = write!(
        out,
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{} test report</title><style>{STYLE}</style></head><body><main>",
        escape(&report.title)
    );
    let _ = write!(
        out,
        "<h1>{}</h1><p class=\"sub\">{} · started {} · {}</p>",
        escape(&report.title),
        if summary.failed > 0 {
            "Some tests failed"
        } else {
            "All tests passed"
        },
        timestamp(report.started),
        duration(report.duration_ms)
    );
    let _ = write!(
        out,
        "<div class=\"cards\"><div class=\"card\"><b>{}</b><span>total</span></div><div class=\"card passed\"><b>{}</b><span>passed</span></div><div class=\"card failed\"><b>{}</b><span>failed</span></div><div class=\"card\"><b>{}</b><span>skipped</span></div><div class=\"card {verdict}\"><b>{:.1}%</b><span>pass rate</span></div></div>{}",
        summary.total(),
        summary.passed,
        summary.failed,
        summary.skipped,
        if summary.total() == 0 {
            100.0
        } else {
            summary.passed as f64 / summary.total() as f64 * 100.0
        },
        bar(summary)
    );
    out.push_str("<div class=\"toolbar\"><input id=\"q\" type=\"search\" placeholder=\"Filter by name or tag\" aria-label=\"Filter\"><button data-filter=\"all\" aria-pressed=\"true\">All</button><button data-filter=\"failed\" aria-pressed=\"false\">Failed</button><button data-filter=\"passed\" aria-pressed=\"false\">Passed</button><button data-filter=\"skipped\" aria-pressed=\"false\">Skipped</button></div>");
    let mut suites: Vec<&Suite> = report.suites.iter().collect();
    suites.sort_by(|a, b| {
        (a.summary().failed == 0, &a.name).cmp(&(b.summary().failed == 0, &b.name))
    });
    for suite in suites {
        suite_section(suite, &mut out);
    }
    let _ = writeln!(out, "</main><script>{SCRIPT}</script></body></html>");
    out
}
