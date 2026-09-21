class Oflh < Formula
  desc "See which processes are using your files"
  homepage "https://github.com/karimz1/open-file-lock-handle"
  license "MIT"
  head "https://github.com/karimz1/open-file-lock-handle.git", branch: "main", using: :git

  depends_on "go" => :build

  def install
    ENV["CGO_ENABLED"] = "0"
    system "go", "build", "-trimpath", "-ldflags", "-s -w -X main.version=dev", "-o", bin/"oflh", "./cmd/oflh"
  end

  test do
    assert_match "oflh dev", shell_output("#{bin}/oflh --version")
  end
end
