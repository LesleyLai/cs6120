pub struct Options {
    pub handle_copy_propagate: bool,
    pub handle_commutativity: bool,
    pub handle_const_folding: bool,
}
impl Options {
    fn new() -> Options {
        Options {
            handle_copy_propagate: false,
            handle_commutativity: false,
            handle_const_folding: false,
        }
    }
}

pub fn parse_options(args: &[String]) -> Options {
    let mut options = Options::new();
    for arg in args {
        if arg == "-p" {
            options.handle_copy_propagate = true;
        } else if arg == "-c" {
            options.handle_commutativity = true;
        } else if arg == "-f" {
            options.handle_copy_propagate = true;
            options.handle_commutativity = true;
            options.handle_const_folding = true;
        }
    }
    options
}
