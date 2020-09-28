# Maintainer: Edward Shen <code@eddie.sh>
#
# You should _always_ use the latest PKGBUILD from master, as each releases
# PKGBUILD will contain the previous release's PKGBUILD. This is because one
# cannot generate the sha512sum of the release until it's been created, and this
# file would be part of said release.

pkgname=bunbun
pkgver=0.8.0
pkgrel=1
depends=('gcc-libs')
makedepends=('rust' 'cargo')
arch=('i686' 'x86_64' 'armv6h' 'armv7h')
pkgdesc="Re-implementation of bunny1 in Rust"
url="https://github.com/edward-shen/bunbun"
license=('AGPL')
source=("$pkgname-$pkgver.tar.gz::https://github.com/edward-shen/$pkgname/archive/$pkgver.tar.gz")
sha512sums=('55ecc42176e57863c87d7196e41f4971694eda7d74200214e2a64b6bb3b54c5990ab224301253317e38b079842318315891159113b6de754cd91171c808660bb')

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
