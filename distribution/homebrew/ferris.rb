class Ferris < Formula
  desc "Sovereign Rust developer platform CLI for ferris.uz"
  homepage "https://ferris.uz"
  url "https://static.crates.io/crates/ferris-cli/ferris-cli-0.2.1.crate"
  sha256 "d977631b87941cd2d04c6eb03f3661adaa0a7756c814faf744c421a71454d747"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/rust-uz/ferris.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
  end

  test do
    assert_match "ferris 0.2.1", shell_output("#{bin}/ferris --version")
  end
end
