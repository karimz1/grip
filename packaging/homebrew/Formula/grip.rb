class Grip < Formula
  desc "See which processes are using your files"
  homepage "https://github.com/karimz1/grip"
  license "MIT"
  head "git@github.com:karimz1/grip.git", branch: "main", using: :git

  depends_on "go" => :build
  conflicts_with "homebrew/core/grip", because: "both install a grip executable"

  def install
    ENV["CGO_ENABLED"] = "0"
    system "go", "build", "-trimpath", "-ldflags", "-s -w -X main.version=dev", "-o", bin/"grip", "./cmd/grip"
  end

  test do
    assert_match "grip dev", shell_output("#{bin}/grip --version")
  end
end
