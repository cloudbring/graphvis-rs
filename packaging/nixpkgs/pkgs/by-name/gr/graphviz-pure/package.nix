{
  lib,
  rustPlatform,
  fetchFromGitHub,
  versionCheckHook,
}:

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "graphviz-pure";
  version = "0.1.0";

  __structuredAttrs = true;

  src = fetchFromGitHub {
    owner = "graphviz-pure";
    repo = "graphviz-pure";
    tag = "v${finalAttrs.version}";
    # TODO(release): replace with the hash nix reports (see packaging/README.md).
    hash = lib.fakeHash;
  };

  # TODO(release): replace with the hash nix reports (see packaging/README.md).
  cargoHash = lib.fakeHash;

  # The repository is a workspace; build, test and install only the command line (the other
  # binary, gv-diff, is the project's differential test harness).
  cargoBuildFlags = [
    "--package"
    "graphviz-pure-cli"
  ];
  # The command line's own tests (offline, a few seconds). The workspace's fidelity ratchets
  # compare against C's outputs on about 2,600 inputs and are left to upstream CI.
  cargoTestFlags = [
    "--package"
    "graphviz-pure-cli"
  ];

  nativeInstallCheckInputs = [ versionCheckHook ];
  versionCheckProgramArg = "-V";
  doInstallCheck = true;

  meta = {
    description = "Byte-exact Rust port of Graphviz 16.1.0 with a dot-compatible command line";
    longDescription = ''
      graphviz-pure is a function-by-function port of Graphviz 16.1.0 to safe Rust, with all
      eight layout engines and the svg, json, dot, xdot, plain, png and jpg outputs. It is an
      independent port, not affiliated with or endorsed by the Graphviz project.
    '';
    homepage = "https://graphviz-pure.github.io/";
    changelog = "https://github.com/graphviz-pure/graphviz-pure/blob/v${finalAttrs.version}/CHANGELOG.md";
    license = with lib.licenses; [
      epl20 # the port
      ofl # the Tinos, Arimo and Cousine fonts compiled into the binary
      mit # the CORE-MATH pow port (see NOTICE)
    ];
    # TODO(submission): add the maintainer entry (maintainers/maintainer-list.nix) in the same PR.
    maintainers = with lib.maintainers; [ ];
    mainProgram = "graphviz-pure";
    platforms = lib.platforms.all;
  };
})
