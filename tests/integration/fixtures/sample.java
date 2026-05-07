import java.util.List;

interface Greeter {
    String greet(String name);
}

class Hello implements Greeter {
    @Override
    public String greet(String name) {
        return "Hello, " + name + "!";
    }
}

class GreetingFactory {
    public static String makeGreeting(String name) {
        Greeter g = new Hello();
        return g.greet(name);
    }
}
