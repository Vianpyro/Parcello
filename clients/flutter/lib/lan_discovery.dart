import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';

import 'session.dart';

/// Simple LAN browser that listens for multicase announcements and shows
/// discovered Parcello servers. Defaults match the server defaults.
class LanBrowser extends StatefulWidget {
  final GameSession session;
  const LanBrowser({super.key, required this.session});

  @override
  State<LanBrowser> createState() => _LanBrowserState();
}

class _LanBrowserState extends State<LanBrowser> {
  static const String maddr = '239.255.0.1';
  static const int port = 55888;

  RawDatagramSocket? _socket;
  final Map<String, Map<String, dynamic>> _servers = {};
  Timer? _cleanup;

  @override
  void initState() {
    super.initState();
    _startListening();
    _cleanup = Timer.periodic(const Duration(seconds: 5), (_) => _prune());
  }

  Future<void> _startListening() async {
    try {
      final sock = await RawDatagramSocket.bind(InternetAddress.anyIPv4, port,
          reuseAddress: true, reusePort: false);
      try {
        sock.joinMulticast(InternetAddress(maddr));
      } catch (_) {}
      sock.listen(_onSocketEvent);
      setState(() => _socket = sock);
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('LAN listen failed: $e')));
      }
    }
  }

  void _onSocketEvent(RawSocketEvent event) {
    if (event != RawSocketEvent.read) return;
    final s = _socket;
    if (s == null) return;
    final dg = s.receive();
    if (dg == null) return;
    try {
      final jsonStr = utf8.decode(dg.data);
      final m = jsonDecode(jsonStr) as Map<String, dynamic>;
      if (m['proto'] != 'parcello-discovery-v1') return;
      final key = '${dg.address.address}:${m['bind'] ?? ''}';
      final entry = {
        'addr': dg.address.address,
        'bind': m['bind'] ?? '',
        'ts': m['ts'] ?? 0,
        'app': m['app'] ?? '',
        'last': DateTime.now().millisecondsSinceEpoch,
      };
      setState(() => _servers[key] = entry);
    } catch (_) {}
  }

  void _prune() {
    final now = DateTime.now().millisecondsSinceEpoch;
    final keys = List<String>.from(_servers.keys);
    var changed = false;
    for (final k in keys) {
      final last = _servers[k]?['last'] as int? ?? 0;
      if (now - last > 15000) {
        _servers.remove(k);
        changed = true;
      }
    }
    if (changed && mounted) setState(() {});
  }

  @override
  void dispose() {
    _cleanup?.cancel();
    _socket?.close();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final list = _servers.values.toList()
      ..sort((a, b) => (b['ts'] as int).compareTo(a['ts'] as int));
    return Scaffold(
      appBar: AppBar(title: const Text('LAN servers')),
      body: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(children: [
          const Text('Detected servers on the local network'),
          const SizedBox(height: 8),
          Expanded(
            child: ListView.builder(
              itemCount: list.length,
              itemBuilder: (ctx, i) {
                final e = list[i];
                final bind = e['bind'] as String;
                final host = e['addr'] as String;
                final label = '${e['app']} at $host ($bind)';
                return ListTile(
                  title: Text(label),
                  subtitle: Text('announced ${e['ts']}'),
                  trailing: FilledButton(
                    child: const Text('Connect'),
                    onPressed: () {
                      final url = bind.isNotEmpty && bind.contains(':')
                          ? 'ws://$bind/ws'
                          : 'ws://$host:7878/ws';
                        widget.session.connect(url, widget.session.authName,
                          token: widget.session.authToken);
                      Navigator.pop(context);
                    },
                  ),
                );
              },
            ),
          ),
        ]),
      ),
    );
  }
}
