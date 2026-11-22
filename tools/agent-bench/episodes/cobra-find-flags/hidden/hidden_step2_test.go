package cobra

import (
	"bytes"
	"testing"
)

func TestHiddenStep2ShortStringFlagBeforeSubcommand(t *testing.T) {
	root, sub := hiddenTree()
	found, _, err := root.Find([]string{"-n", "x", "sub"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if found != sub {
		t.Fatalf("expected sub, found %q", found.Name())
	}
}

func TestHiddenStep2ShortBoolFlagBeforeSubcommand(t *testing.T) {
	root, sub := hiddenTree()
	found, _, err := root.Find([]string{"-v", "sub"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if found != sub {
		t.Fatalf("expected sub, found %q", found.Name())
	}
}

func TestHiddenStep2ExecuteRunsSubcommandWithMixedFlags(t *testing.T) {
	root, sub := hiddenTree()
	ran := ""
	root.Run = func(*Command, []string) { ran = "root" }
	sub.Run = func(*Command, []string) { ran = "sub" }
	root.SetOut(&bytes.Buffer{})
	root.SetErr(&bytes.Buffer{})
	root.SetArgs([]string{"--config", "x", "-n", "y", "-v", "sub"})
	if err := root.Execute(); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if ran != "sub" {
		t.Fatalf("expected sub to run, ran %q", ran)
	}
}
