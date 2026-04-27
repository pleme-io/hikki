{
  description = "Hikki (筆記) — GPU notes app for macOS and Linux";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-25.11";
    crate2nix.url = "github:nix-community/crate2nix";
    flake-utils.url = "github:numtide/flake-utils";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crate2nix,
    flake-utils,
    substrate,
  }:
    (import "${substrate}/lib/rust-tool-release-flake.nix" {
      inherit nixpkgs crate2nix flake-utils;
    }) {
      toolName = "hikki";
      src = self;
      repo = "pleme-io/hikki";

      # Migration to substrate module-trio + shikumiTypedGroups.
      # See kekkai for the canonical template; hikki is the second
      # example showing how to:
      #   1. use `enum` types via raw `lib.types.enum` in field.type
      #   2. wire a daemon gated on a non-default option (sync.enable
      #      instead of daemon.enable) via extraHmConfigFn rather than
      #      withUserDaemon
      module = {
        description = "Hikki (筆記) — GPU notes app for macOS and Linux";
        hmNamespace = "blackmatter.components";

        # No withUserDaemon — hikki gates the daemon on sync.enable, so
        # the daemon wiring lives in extraHmConfigFn below. (The trio's
        # withUserDaemon hard-codes `cfg.daemon.enable` as the gate.)

        # Shikumi YAML config at ~/.config/hikki/hikki.yaml.
        withShikumiConfig = true;

        shikumiTypedGroups = {
          appearance = {
            width        = { type = "int";   default = 800;  description = "Window width in pixels."; };
            height       = { type = "int";   default = 600;  description = "Window height in pixels."; };
            font_size    = { type = "float"; default = 15.0; description = "Font size in points."; };
            opacity      = { type = "float"; default = 0.95; description = "Background opacity (0.0-1.0)."; };
            line_spacing = { type = "float"; default = 1.5;  description = "Line spacing multiplier."; };
          };

          editor = {
            tab_size       = { type = "int";  default = 4;    description = "Tab size in spaces."; };
            word_wrap      = { type = "bool"; default = true; description = "Enable word wrap."; };
            spell_check    = { type = "bool"; default = true; description = "Enable spell checking."; };
            auto_save_secs = { type = "int";  default = 30;   description = "Auto-save interval in seconds (0 to disable)."; };
          };

          storage = {
            notes_dir   = { type = "str";  default = "~/Documents/hikki"; description = "Directory where notes are stored."; };
            format      = {
              # Raw types.* expression — the alias dictionary doesn't
              # cover enum (since the value list varies). Pass the
              # type directly when an alias doesn't fit.
              type        = nixpkgs.lib.types.enum [ "markdown" ];
              default     = "markdown";
              description = "Note file format.";
            };
            auto_backup = { type = "bool"; default = true; description = "Enable automatic backups."; };
          };

          search = {
            index_on_save = { type = "bool"; default = true; description = "Re-index notes on every save."; };
            max_results   = { type = "int";  default = 50;   description = "Maximum search results to return."; };
          };

          sync = {
            enable     = { type = "bool"; default = false; description = "Enable note synchronization."; };
            method     = {
              type        = nixpkgs.lib.types.enum [ "git" "icloud" ];
              default     = "git";
              description = "Sync method.";
            };
            remote_url = { type = "nullOrStr"; default = null; description = "Remote URL for git sync."; };
          };
        };

        # Bespoke escape hatch — extraSettings merged into the YAML.
        extraHmOptions = {
          extraSettings = nixpkgs.lib.mkOption {
            type = nixpkgs.lib.types.attrs;
            default = { };
            description = "Additional raw settings merged on top of the typed YAML.";
          };
        };

        # Wire the sync daemon gated on cfg.sync.enable. Uses
        # substrate's hmHelpers (same import as the legacy module/
        # default.nix did).
        extraHmConfigFn =
          { cfg, pkgs, lib, config, ... }:
          let
            hmHelpers = import "${substrate}/lib/hm/service-helpers.nix" {
              inherit lib;
            };
            isDarwin = pkgs.stdenv.hostPlatform.isDarwin;
            logDir =
              if isDarwin then "${config.home.homeDirectory}/Library/Logs"
              else "${config.home.homeDirectory}/.local/share/hikki/logs";
            daemonArgs = [ "search" "--daemon" ];
            extras = cfg.extraSettings;
          in lib.mkMerge [
            # Merge extraSettings into the YAML payload.
            (lib.mkIf (extras != { }) {
              services.hikki.settings = extras;
            })

            # Log directory — keeps parity with the legacy module's
            # entryAfter activation hook.
            {
              home.activation.hikki-log-dir =
                lib.hm.dag.entryAfter [ "writeBoundary" ] ''
                  run mkdir -p "${logDir}"
                '';
            }

            # Sync daemon, gated on sync.enable.
            (lib.mkIf (cfg.sync.enable && isDarwin)
              (hmHelpers.mkLaunchdService {
                name = "hikki";
                label = "io.pleme.hikki";
                command = "${cfg.package}/bin/hikki";
                args = daemonArgs;
                logDir = logDir;
                processType = "Background";
                keepAlive = true;
              }))

            (lib.mkIf (cfg.sync.enable && !isDarwin)
              (hmHelpers.mkSystemdService {
                name = "hikki";
                description = "Hikki — notes sync daemon";
                command = "${cfg.package}/bin/hikki";
                args = daemonArgs;
              }))
          ];
      };
    };
}
