#[cfg(all(feature = "jemalloc-wall", feature = "dhat-heap"))]
compile_error!("jemalloc-wall and dhat-heap are mutually exclusive");

fn main() {
    eprintln!("project-local binder measurement probe is not implemented");
    std::process::exit(2);
}
