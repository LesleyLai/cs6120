pub struct Options {
    pub handle_copy_propagate: bool,
}
impl Options {
    fn new() -> Options {
        Options {
            handle_copy_propagate: false,
        }
    }
}

pub fn parse_options(args: &[String]) -> Options {
    let mut options = Options::new();
    for arg in args {
        if arg == "-p" {
            options.handle_copy_propagate = true;
        }
    }
    options
}
