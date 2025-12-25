# Homebrew Formula for nvim-web
#
# To use:
# 1. Copy to your tap: $(brew --repository)/Library/Taps/your-username/homebrew-tap/Formula/nvim-web.rb
# 2. Or install directly: brew install --build-from-source ./nvim-web.rb

class NvimWeb < Formula
  desc "Neovim in the Browser - WebSocket bridge for browser-based Neovim"
  homepage "https://github.com/your-username/nvim-web"
  url "https://github.com/your-username/nvim-web/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "PLACEHOLDER_SHA256"
  license "MIT"

  depends_on "rust" => :build
  depends_on "neovim"

  def install
    cd "host" do
      system "cargo", "build", "--release"
      bin.install "target/release/nvim-web-host" => "nvim-web"
    end

    # Install UI files
    (share/"nvim-web/ui").install Dir["ui/*"]
    
    # Install plugin
    (share/"nvim-web/plugin").install Dir["plugin/*"]
  end

  def caveats
    <<~EOS
      To start nvim-web:
        nvim-web

      To serve the UI:
        cd #{share}/nvim-web/ui && python3 -m http.server 8080

      Then open http://localhost:8080

      To install the Neovim plugin, add to your config:
        vim.opt.runtimepath:append("#{share}/nvim-web/plugin")
        require("nvim-web").setup()
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
