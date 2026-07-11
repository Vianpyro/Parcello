/// `LanBrowser`: UDP multicast LAN server discovery (ADR-0016) on native
/// targets, a "not available" stub on web (no raw socket API in a browser
/// sandbox) - see `lan_discovery_io.dart` / `lan_discovery_stub.dart`.
library;

export 'lan_discovery_stub.dart' if (dart.library.io) 'lan_discovery_io.dart';
