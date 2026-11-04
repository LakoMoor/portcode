# Homebrew formula template. Publish in a tap repo (LakoMoor/homebrew-tap, Formula/portcode.rb):
#   brew tap LakoMoor/tap && brew install portcode
# Fill version and sha256 values with: packaging/homebrew/update.sh vX.Y.Z
class Portcode < Formula
  desc "Port anything to Rust, with proof: terminal agent for local and self-hosted LLMs"
  homepage "https://lakomoor.github.io/portcode/"
  version "0.1.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/LakoMoor/portcode/releases/download/v0.1.1/portcode-aarch64-apple-darwin.tar.gz"
      sha256 "abe7450f5824d5d18b773c61baa595f6e887c3c742bfd17582d2309ad83b703c"
    end
    on_intel do
      url "https://github.com/LakoMoor/portcode/releases/download/v0.1.1/portcode-x86_64-apple-darwin.tar.gz"
      sha256 "54b4d77762c232008c643787893c8d6a25dd2a29a9647e939750b4c2d3be86f6"
    end
  end
  on_linux do
    on_arm do
      url "https://github.com/LakoMoor/portcode/releases/download/v0.1.1/portcode-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "9a5910f8f4c35e56f92e365e2a7142f0d228d130e312245965b3fcef06411196"
    end
    on_intel do
      url "https://github.com/LakoMoor/portcode/releases/download/v0.1.1/portcode-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "6cd84afa4945870cc7bd67729dc08315c1e8e0022650ab0d553608836736bdbe"
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
