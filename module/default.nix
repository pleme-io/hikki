# Hikki home-manager module — GPU notes app with typed config + daemon
#
# Namespace: blackmatter.components.hikki.*
#
# Generates YAML config from typed Nix options, loaded by shikumi at runtime.
# Supports hot-reload via symlink-aware file watching.
#
# Module factory: receives { hmHelpers } from flake.nix, returns HM module.
{ hmHelpers }:
{
  lib,
  config,
  pkgs,
  ...
}:
with lib;
let
  inherit (hmHelpers) mkLaunchdService mkSystemdService;
  cfg = config.blackmatter.components.hikki;
  isDarwin = pkgs.stdenv.isDarwin;

  logDir =
    if isDarwin then "${config.home.homeDirectory}/Library/Logs"
    else "${config.home.homeDirectory}/.local/share/hikki/logs";

  # -- YAML config generation --------------------------------------------------
  settingsAttr = let
    appearance = filterAttrs (_: v: v != null) {
      inherit (cfg.appearance) width height font_size opacity line_spacing;
    };

    editor = filterAttrs (_: v: v != null) {
      inherit (cfg.editor) tab_size word_wrap spell_check auto_save_secs;
    };

    storage = filterAttrs (_: v: v != null) {
      notes_dir = cfg.storage.notes_dir;
      format = cfg.storage.format;
      auto_backup = cfg.storage.auto_backup;
    };

    search = filterAttrs (_: v: v != null) {
      inherit (cfg.search) index_on_save max_results;
    };

    sync = optionalAttrs cfg.sync.enable (filterAttrs (_: v: v != null) {
      enable = cfg.sync.enable;
      method = cfg.sync.method;
      remote_url = cfg.sync.remote_url;
    });
  in
    filterAttrs (_: v: v != {} && v != null) {
      inherit appearance editor storage search sync;
    }
    // cfg.extraSettings;

  yamlConfig = pkgs.writeText "hikki.yaml"
    (lib.generators.toYAML { } settingsAttr);
in
{
  options.blackmatter.components.hikki = {
    enable = mkEnableOption "Hikki — GPU notes app";

    package = mkOption {
      type = types.package;
      default = pkgs.hikki;
      description = "The hikki package to use.";
    };

    # -- Appearance ------------------------------------------------------------
    appearance = {
      width = mkOption {
        type = types.int;
        default = 800;
        description = "Window width in pixels.";
      };

      height = mkOption {
        type = types.int;
        default = 600;
        description = "Window height in pixels.";
      };

      font_size = mkOption {
        type = types.float;
        default = 15.0;
        description = "Font size in points.";
      };

      opacity = mkOption {
        type = types.float;
        default = 0.95;
        description = "Background opacity (0.0-1.0).";
      };

      line_spacing = mkOption {
        type = types.float;
        default = 1.5;
        description = "Line spacing multiplier.";
      };
    };

    # -- Editor ----------------------------------------------------------------
    editor = {
      tab_size = mkOption {
        type = types.int;
        default = 4;
        description = "Tab size in spaces.";
      };

      word_wrap = mkOption {
        type = types.bool;
        default = true;
        description = "Enable word wrap.";
      };

      spell_check = mkOption {
        type = types.bool;
        default = true;
        description = "Enable spell checking.";
      };

      auto_save_secs = mkOption {
        type = types.int;
        default = 30;
        description = "Auto-save interval in seconds (0 to disable).";
      };
    };

    # -- Storage ---------------------------------------------------------------
    storage = {
      notes_dir = mkOption {
        type = types.str;
        default = "~/Documents/hikki";
        description = "Directory where notes are stored.";
      };

      format = mkOption {
        type = types.enum [ "markdown" ];
        default = "markdown";
        description = "Note file format.";
      };

      auto_backup = mkOption {
        type = types.bool;
        default = true;
        description = "Enable automatic backups.";
      };
    };

    # -- Search ----------------------------------------------------------------
    search = {
      index_on_save = mkOption {
        type = types.bool;
        default = true;
        description = "Re-index notes on every save.";
      };

      max_results = mkOption {
        type = types.int;
        default = 50;
        description = "Maximum search results to return.";
      };
    };

    # -- Sync ------------------------------------------------------------------
    sync = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = "Enable note synchronization.";
      };

      method = mkOption {
        type = types.enum [ "git" "icloud" ];
        default = "git";
        description = "Sync method.";
      };

      remote_url = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Remote URL for git sync.";
        example = "git@github.com:user/notes.git";
      };
    };

    # -- Escape hatch ----------------------------------------------------------
    extraSettings = mkOption {
      type = types.attrs;
      default = {};
      description = ''
        Additional raw settings merged on top of typed options.
        Use this for experimental or newly-added config keys not yet
        covered by typed options. Values are serialized directly to YAML.
      '';
      example = {
        experimental = {
          gpu_backend = "metal";
        };
      };
    };
  };

  config = mkIf cfg.enable (mkMerge [
    # Install the package
    {
      home.packages = [ cfg.package ];
    }

    # Create log directory
    {
      home.activation.hikki-log-dir = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
        run mkdir -p "${logDir}"
      '';
    }

    # YAML configuration -- always generated from typed options
    {
      xdg.configFile."hikki/hikki.yaml".source = yamlConfig;
    }

    # Darwin: launchd agent (sync mode)
    (mkIf (cfg.sync.enable && isDarwin)
      (mkLaunchdService {
        name = "hikki";
        label = "io.pleme.hikki";
        command = "${cfg.package}/bin/hikki";
        args = [ "search" "--daemon" ];
        logDir = logDir;
        processType = "Background";
        keepAlive = true;
      })
    )

    # Linux: systemd user service (sync mode)
    (mkIf (cfg.sync.enable && !isDarwin)
      (mkSystemdService {
        name = "hikki";
        description = "Hikki — notes sync daemon";
        command = "${cfg.package}/bin/hikki";
        args = [ "search" "--daemon" ];
      })
    )
  ]);
}
