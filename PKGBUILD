# Maintainer: Edward Shen <code@eddie.sh>
pkgname=bunbun
pkgver=0.4.1
pkgrel=1
depends=('gcc-libs')
makedepends=('rust' 'cargo')
arch=('i686' 'x86_64' 'armv6h' 'armv7h')
pkgdesc="Re-implementation of bunny1 in Rust"
url="https://github.com/edward-shen/bunbun"
license=('AGPL')
source=("$pkgname-$pkgver.tar.gz::https://github.com/edward-shen/$pkgname/archive/$pkgver.tar.gz")
sha512sums=('b8576b40e1912bb651b12a8adb591c93fe60116ea5870a77852789fcd3e5027438847120cd1f2549271365de1ba1387145d026c035473ab510e3628fe791458e')

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
  install -Dm644 "$srcdir/aux/systemd/$pkgname.service" -t "$pkgdir/usr/lib/systemd/system"
  install -Dm644 "$srcdir/$pkgname.default.yaml" "$pkgdir/etc/$pkgname.yaml"
}
