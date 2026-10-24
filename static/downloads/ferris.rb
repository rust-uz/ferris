class Ferris < Formula
  desc "Sovereign Rust developer platform CLI for ferris.uz"
  homepage "https://ferris.uz"
  url "https://static.crates.io/crates/ferris-cli/ferris-cli-0.3.0.crate"
  sha256 "e605f400a46a98d245c46ea51739ffb4442fa3bb79716762c06c135adf0aedd6"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/rust-uz/ferris.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
  end

  test do
    assert_match "ferris 0.3.0", shell_output("#{bin}/ferris --version")
  end
end
