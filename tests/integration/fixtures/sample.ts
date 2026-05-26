interface Greeter {
  greet(name: string): string;
}

class Hello implements Greeter {
  greet(name: string): string {
    return `Hello, ${name}!`;
  }
}

class FancyGreeter extends Hello {
  greet(name: string): string {
    return `Greetings, ${name}!`;
  }
}

function makeGreeting(name: string): string {
  const g = new Hello();
  return g.greet(name);
}
