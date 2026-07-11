/// Web stub for `LanBrowser`: LAN multicast discovery has no browser
/// equivalent (no raw socket API), so `main.dart` never navigates here on
/// web (`kIsWeb` guard). Kept only so the app compiles for the web target;
/// selected by the conditional export in `lan_discovery.dart`.
library;

import 'package:flutter/material.dart';

import 'session.dart';

class LanBrowser extends StatelessWidget {
  final GameSession session;
  const LanBrowser({super.key, required this.session});

  @override
  Widget build(BuildContext context) => Scaffold(
        appBar: AppBar(title: const Text('LAN servers')),
        body: const Center(
          child: Text('LAN discovery is not available in the browser.'),
        ),
      );
}
