# Homebrew Formula for nvim-web
#
# To use:
# 1. Copy to your tap: $(brew --repository)/Library/Taps/your-username/homebrew-tap/Formula/nvim-web.rb
# 2. Or install directly: brew install --build-from-source ./nvim-web.rb

class NvimWeb < Formula
  desc "Neovim in the Browser - Real Neovim via WebSocket"
  homepage "https://github.com/kj114022/nvim-web"
  url "https://github.com/kj114022/nvim-web/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "PLACEHOLDER_SHA256"
  license "MIT"

  depends_on "rust" => :build
  depends_on "neovim"

  def install
    # Build from workspace root
    system "cargo", "build", "--release", "-p", "nvim-web-host"
    bin.install "target/release/nvim-web-host" => "nvim-web"
  end

  def caveats
    <<~EOS
      nvim-web is a single binary with all UI assets embedded.

      To start the server:
        nvim-web

      Then open http://localhost:8080 in your browser.

      To open a project directly:
        nvim-web open /path/to/project
    EOS
  end

  service do
    run [opt_bin/"nvim-web"]
    keep_alive true
    working_dir var/"nvim-web"
    log_path var/"log/nvim-web.log"
    error_log_path var/"log/nvim-web.log"
  end

  test do
    assert_match "nvim-web", shell_output("#{bin}/nvim-web --version")
  end
end
