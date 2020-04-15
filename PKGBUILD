# Maintainer: Edward Shen <code@eddie.sh>
#
# You should _always_ use the latest PKGBUILD from master, as each releases
# PKGBUILD will contain the previous release's PKGBUILD. This is because one
# cannot generate the sha512sum of the release until it's been created, and this
# file would be part of said release.

pkgname=bunbun
pkgver=0.6.1
pkgrel=1
depends=('gcc-libs')
makedepends=('rust' 'cargo')
arch=('i686' 'x86_64' 'armv6h' 'armv7h')
pkgdesc="Re-implementation of bunny1 in Rust"
url="https://github.com/edward-shen/bunbun"
license=('AGPL')
source=("$pkgname-$pkgver.tar.gz::https://github.com/edward-shen/$pkgname/archive/$pkgver.tar.gz")
sha512sums=('f3781cbb4e9e190df38c3fe7fa80ba69bf6f9dbafb158e0426dd4604f2f1ba794450679005a38d0f9f1dad0696e2f22b8b086b2d7d08a0f99bb4fd3b0f7ed5d8')

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
