fn main() {
    std::process::exit(quench_tool::run(std::env::args_os().skip(1)));
}
