# Homebrew formula template. Publish in a tap repo (LakoMoor/homebrew-tap, Formula/portcode.rb):
#   brew tap LakoMoor/tap && brew install portcode
# Fill version and sha256 values with: packaging/homebrew/update.sh vX.Y.Z
class Portcode < Formula
  desc "Port anything to Rust, with proof: terminal agent for local and self-hosted LLMs"
  homepage "https://lakomoor.github.io/portcode/"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/LakoMoor/portcode/releases/download/v0.1.0/portcode-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_AARCH64_APPLE_DARWIN"
    end
    on_intel do
      url "https://github.com/LakoMoor/portcode/releases/download/v0.1.0/portcode-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_X86_64_APPLE_DARWIN"
    end
  end
  on_linux do
    on_arm do
      url "https://github.com/LakoMoor/portcode/releases/download/v0.1.0/portcode-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_AARCH64_LINUX"
    end
    on_intel do
      url "https://github.com/LakoMoor/portcode/releases/download/v0.1.0/portcode-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_X86_64_LINUX"
    end
  end

  def install
    bin.install "portcode"
    generate_completions_from_executable(bin/"portcode", "completions")
  end

  test do
    assert_match "portcode", shell_output("#{bin}/portcode --version")
  end
end
