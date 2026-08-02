# Homebrew release setup

GitCortex uses cargo-dist to generate `gitcortex.rb` from the macOS and Linux
release archives and publish it to the official Homebrew tap.

## One-time repository-owner setup

1. Create the public repository `bharath03-a/homebrew-tap` with a `Formula/`
   directory on its default branch.
2. Create a fine-grained GitHub token that can write repository contents only
   in `bharath03-a/homebrew-tap`.
3. Add the token to the GitCortex repository as the Actions secret
   `HOMEBREW_TAP_TOKEN`.
4. Run the release workflow once and confirm that it commits
   `Formula/gitcortex.rb` to the tap.

After that, users install or update GitCortex with:

```bash
brew install bharath03-a/tap/gitcortex
brew upgrade gitcortex
```

The release workflow does not publish prerelease formulas unless cargo-dist's
`publish-prereleases` setting is enabled. Pull requests still build the formula
as an artifact, so formula generation is validated before a tagged release.
