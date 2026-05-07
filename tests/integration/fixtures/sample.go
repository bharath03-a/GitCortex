package greeter

type Greeter interface {
	Greet(name string) string
}

type Hello struct {
	Prefix string
}

func (h *Hello) Greet(name string) string {
	return h.Prefix + ", " + name + "!"
}

func MakeGreeting(name string) string {
	h := &Hello{Prefix: "Hello"}
	return h.Greet(name)
}
