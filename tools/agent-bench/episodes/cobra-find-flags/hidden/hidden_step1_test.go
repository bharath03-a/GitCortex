package cobra

import "testing"

func hiddenTree() (*Command, *Command) {
	root := &Command{Use: "root", Run: func(*Command, []string) {}}
	sub := &Command{Use: "sub", Run: func(*Command, []string) {}}
	root.PersistentFlags().String("config", "", "config file")
	root.PersistentFlags().StringP("name", "n", "", "name")
	root.PersistentFlags().BoolP("verbose", "v", false, "verbose")
	root.AddCommand(sub)
	return root, sub
}

func TestHiddenStep1LongStringFlagBeforeSubcommand(t *testing.T) {
	root, sub := hiddenTree()
	found, _, err := root.Find([]string{"--config", "x", "sub"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if found != sub {
		t.Fatalf("expected sub, found %q", found.Name())
	}
}

func TestHiddenStep1LongBoolFlagBeforeSubcommand(t *testing.T) {
	root, sub := hiddenTree()
	found, _, err := root.Find([]string{"--verbose", "sub"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if found != sub {
		t.Fatalf("expected sub, found %q", found.Name())
	}
}

func TestHiddenStep1LongFlagValueIsNotACommand(t *testing.T) {
	root, _ := hiddenTree()
	got := stripFlags([]string{"--config", "sub", "sub"}, root)
	if len(got) != 1 || got[0] != "sub" {
		t.Fatalf("expected only the real subcommand, got %v", got)
	}
}
