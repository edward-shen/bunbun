# Maintainer: Edward Shen <code@eddie.sh>
#
# You should _always_ use the latest PKGBUILD from master, as each releases
# PKGBUILD will contain the previous release's PKGBUILD. This is because one
# cannot generate the sha512sum of the release until it's been created, and this
# file would be part of said release.

pkgname=bunbun
pkgver=0.6.2
pkgrel=1
depends=('gcc-libs')
makedepends=('rust' 'cargo')
arch=('i686' 'x86_64' 'armv6h' 'armv7h')
pkgdesc="Re-implementation of bunny1 in Rust"
url="https://github.com/edward-shen/bunbun"
license=('AGPL')
source=("$pkgname-$pkgver.tar.gz::https://github.com/edward-shen/$pkgname/archive/$pkgver.tar.gz")
sha512sums=('97588e9c336e4f6c10bb1de7fefa02fede27114060f0b36e7f331e51e89ee1e211578aec8754a7fac8669bf63fef008b24ef9ef507f5854db0e26129bab8719c')

build() {
  cd "$pkgname-$pkgver"

  cargo build --release --locked
  strip "target/release/$pkgname" || true
}

check() {
  cd "$pkgname-$pkgver"

  cargo test --release --locked
}

package() {
  cd "$pkgname-$pkgver"

  install -Dm755 "target/release/$pkgname" -t "$pkgdir/usr/bin"
  install -Dm644 "aux/systemd/$pkgname.service" -t "$pkgdir/usr/lib/systemd/system"
  install -Dm644 "$pkgname.default.yaml" "$pkgdir/etc/$pkgname.yaml"
}
