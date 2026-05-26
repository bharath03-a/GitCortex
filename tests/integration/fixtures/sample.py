class Greeter:
    def greet(self, name: str) -> str:
        return f"Hello, {name}!"


class FancyGreeter(Greeter):
    def greet(self, name: str) -> str:
        return f"Greetings, {name}!"


def make_greeting(name: str) -> str:
    g = Greeter()
    return g.greet(name)
