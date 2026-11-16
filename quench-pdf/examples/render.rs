//! `cargo run -p quench-pdf --example render -- in.pdf out_prefix`: every page as `out_prefix-<n>.svg`.
#![allow(
    clippy::print_stdout,
    clippy::format_push_string,
    clippy::suboptimal_flops
)]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&args[1]).expect("read the file");
    let doc = quench_pdf::Document::load(&data).expect("a PDF");
    for i in 0..doc.page_count() {
        let page = doc.render_page(i).expect("a page");
        std::fs::write(format!("{}-{}.svg", args[2], i + 1), page.svg).expect("write");
        println!("page {} {}x{}", i + 1, page.width, page.height);
        // A page of HTML with the text laid over the picture, to see how well they line up.
        let k = 1.3333;
        let mut html = format!(
            "<!doctype html><meta charset=utf-8><style>.w{{position:relative;width:{}px;height:{}px}}.r{{position:absolute;overflow:hidden;white-space:pre;line-height:1;color:transparent;text-align:justify;text-align-last:justify;font-family:'Liberation Sans',Arial,sans-serif;background:rgba(255,0,0,.25)}}</style><div class=w><img src=\"{}-{}.svg\" width={}>",
            page.width * k,
            page.height * k,
            args[2].rsplit('/').next().unwrap_or(""),
            i + 1,
            page.width * k
        );
        for r in &page.runs {
            html.push_str(&format!(
                "<span class=r style=\"left:{:.1}px;top:{:.1}px;width:{:.1}px;height:{:.1}px;font-size:{:.1}px\">{}</span>",
                r.x * k,
                (r.y - r.size * 0.85) * k,
                r.width * k,
                r.size * k,
                r.size * k,
                r.text.replace('&', "&amp;").replace('<', "&lt;")
            ));
        }
        html.push_str("</div>");
        std::fs::write(format!("{}-{}.html", args[2], i + 1), html).expect("write");
    }
}
