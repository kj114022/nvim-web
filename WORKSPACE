# Bazel WORKSPACE for nvim-web
# Uses rules_rust with crates_universe for Cargo.toml integration

workspace(name = "nvim_web")

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

# ----------------------------------------------------------------------------
# rules_rust - Rust build rules
# ----------------------------------------------------------------------------

RULES_RUST_VERSION = "0.57.1"
RULES_RUST_SHA256 = "09a72cf1ac96e5a51f9ddae87bd52abc5a3959c2da1f3df69697def41d7b7e0c"

http_archive(
    name = "rules_rust",
    sha256 = RULES_RUST_SHA256,
    urls = ["https://github.com/bazelbuild/rules_rust/releases/download/{version}/rules_rust-{version}.tar.gz".format(version = RULES_RUST_VERSION)],
)

load("@rules_rust//rust:repositories.bzl", "rules_rust_dependencies", "rust_register_toolchains")

rules_rust_dependencies()

rust_register_toolchains(
    edition = "2021",
    versions = ["1.83.0"],
)

# ----------------------------------------------------------------------------
# crates_universe - Generate Bazel targets from Cargo.toml
# ----------------------------------------------------------------------------

load("@rules_rust//crate_universe:repositories.bzl", "crate_universe_dependencies")

crate_universe_dependencies()

load("@rules_rust//crate_universe:defs.bzl", "crates_repository")

crates_repository(
    name = "crate_index",
    cargo_lockfile = "//:Cargo.lock",
    lockfile = "//:Cargo.Bazel.lock",
    manifests = [
        "//:Cargo.toml",
        "//crates/host:Cargo.toml",
        "//crates/ui:Cargo.toml",
        "//crates/vfs:Cargo.toml",
        "//crates/protocol:Cargo.toml",
    ],
)

load("@crate_index//:defs.bzl", "crate_repositories")

crate_repositories()

# ----------------------------------------------------------------------------
# rules_pkg - For packaging (deb, rpm, tar)
# ----------------------------------------------------------------------------

http_archive(
    name = "rules_pkg",
    sha256 = "8f9ee2dc10c1ae514ee599a8b42ed99fa262b757058f65ad3c384289ff70c4b8",
    urls = [
        "https://github.com/bazelbuild/rules_pkg/releases/download/0.9.1/rules_pkg-0.9.1.tar.gz",
    ],
)

load("@rules_pkg//:deps.bzl", "rules_pkg_dependencies")

rules_pkg_dependencies()
