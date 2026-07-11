/// `ServerManager`: local `parcello-server` process control on native
/// targets, a "not available" stub on web (a browser page cannot spawn an
/// OS process) - see `server_manager_io.dart` / `server_manager_stub.dart`.
library;

export 'server_manager_stub.dart'
    if (dart.library.io) 'server_manager_io.dart';
