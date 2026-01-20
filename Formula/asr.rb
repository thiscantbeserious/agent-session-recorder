# Homebrew formula for Agent Session Recorder
# To use: copy this file to your homebrew-tap repository
#
# Setup:
# 1. Create repo: github.com/<username>/homebrew-tap
# 2. Copy this file to: Formula/asr.rb
# 3. Update the url and sha256 for each release
#
# Users install with:
#   brew tap <username>/tap
#   brew install asr

class Asr < Formula
  desc "CLI tool for recording AI agent terminal sessions with asciinema"
  homepage "https://github.com/thiscantbeserious/agent-session-record"
  version "0.1.0"
  license "MIT"

  # Update these for each release
  # TODO: Update URL to point to actual release assets
  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/thiscantbeserious/agent-session-record/releases/download/v#{version}/asr-darwin-arm64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_ARM64"
    else
      url "https://github.com/thiscantbeserious/agent-session-record/releases/download/v#{version}/asr-darwin-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_X86_64"
    end
  end

  on_linux do
    url "https://github.com/thiscantbeserious/agent-session-record/releases/download/v#{version}/asr-linux-x86_64.tar.gz"
    sha256 "PLACEHOLDER_SHA256_LINUX"
  end

  depends_on "asciinema"

  def install
    bin.install "asr"
  end

  def post_install
    # Create default directories
    (var/"asr").mkpath
  end

  def caveats
    <<~EOS
      To enable shell integration, add to your ~/.zshrc or ~/.bashrc:
        eval "$(asr shell-init)"

      Or source the shell script directly:
        source #{opt_share}/asr/asr.sh

      Default session directory: ~/recorded_agent_sessions/
      Config file: ~/.config/asr/config.toml
    EOS
  end

  test do
    assert_match "asr", shell_output("#{bin}/asr --version")
  end
end
