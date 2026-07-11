/// Web stub for `ServerManager`: spawning/killing a local OS process has
/// no browser equivalent, so `main.dart` never navigates here on web
/// (`kIsWeb` guard). Kept only so the app compiles for the web target;
/// selected by the conditional export in `server_manager.dart`.
library;

import 'package:flutter/material.dart';

class ServerManager extends StatelessWidget {
  const ServerManager({super.key});

  @override
  Widget build(BuildContext context) => Scaffold(
        appBar: AppBar(title: const Text('Server Manager')),
        body: const Center(
          child: Text('Local server management is not available in the browser.'),
        ),
      );
}
