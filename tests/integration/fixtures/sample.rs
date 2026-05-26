pub trait Greeter {
    fn greet(&self) -> String;
}

pub struct Hello {
    pub name: String,
}

impl Greeter for Hello {
    fn greet(&self) -> String {
        format!("Hello, {}!", self.name)
    }
}

pub fn make_greeting(name: &str) -> String {
    let h = Hello { name: name.to_owned() };
    h.greet()
}
