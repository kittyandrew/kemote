# kemote - kitty emotes picker

@TODO: instructions on how to install and configure with nixos/hyprland.

## Binary cache

Builds are published to `cache.kittyandrew.dev`, so Nix can download these outputs instead of rebuilding them.

On NixOS:

```nix
nix.settings = {
  extra-substituters = ["https://cache.kittyandrew.dev/nix-cache"];
  extra-trusted-public-keys = ["cache.kittyandrew.dev-1:yy5fdErj1riKOjND10kzD5mp0L8/C8RFG3VkMizhGg4="];
};
```

Elsewhere, in `~/.config/nix/nix.conf` (or `/etc/nix/nix.conf` for all users):

```
extra-substituters = https://cache.kittyandrew.dev/nix-cache
extra-trusted-public-keys = cache.kittyandrew.dev-1:yy5fdErj1riKOjND10kzD5mp0L8/C8RFG3VkMizhGg4=
```

The `extra-` prefixes append rather than replace, so `cache.nixos.org` keeps working. The cache is read-only
and needs no credentials; it serves only what this repository's flake builds.
