/// Local `parcello-server` process controller. Native-only - a browser
/// page cannot spawn an OS process. Selected for non-web targets by the
/// conditional export in `server_manager.dart`.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';

import 'sfx.dart';

/// Best-effort default path to the bundled server binary.
///
/// In the packaged all-in-one layout the server sits in `./server` next to
/// the client executable, so resolve against the executable's own directory
/// (robust to whatever working directory Steam / the OS launches us from).
/// Falls back to a bare name - resolved via `PATH`, or edited by hand - for
/// dev builds and custom installs, where the binary lives elsewhere.
String defaultServerBinary() {
  final name = Platform.isWindows ? 'parcello-server.exe' : 'parcello-server';
  try {
    final exeDir = File(Platform.resolvedExecutable).parent.path;
    final bundled = File('$exeDir/server/$name');
    if (bundled.existsSync()) return bundled.path;
  } catch (_) {
    // resolvedExecutable can throw on exotic hosts; the bare name is fine.
  }
  return name;
}

/// Local server controller: spawns a `parcello-server` binary, streams its
/// logs, and stops/restarts it by killing the process. Local-only by design
/// (no remote admin control plane; see ADR-0016 / server main.rs).
class ServerManager extends StatefulWidget {
  const ServerManager({super.key});

  @override
  State<ServerManager> createState() => _ServerManagerState();
}

class _ServerManagerState extends State<ServerManager> {
  final _bin = TextEditingController(text: defaultServerBinary());
  final _args = TextEditingController();

  Process? _process;
  final List<String> _logs = [];
  StreamSubscription<List<int>>? _outSub;
  StreamSubscription<List<int>>? _errSub;

  void _append(String line) {
    setState(() {
      _logs.add(line);
      if (_logs.length > 1000) _logs.removeRange(0, _logs.length - 1000);
    });
  }

  Future<void> _startProcess() async {
    if (_process != null) return;
    try {
      final args = _args.text.trim().isEmpty
          ? <String>[]
          : _args.text.split(RegExp(r'\s+'));
      _append('Starting ${_bin.text} ${args.join(' ')}');
      final proc = await Process.start(_bin.text, args);
      _process = proc;
      _outSub = proc.stdout.listen((b) => _append(utf8.decode(b)));
      _errSub = proc.stderr.listen((b) => _append(utf8.decode(b)));
      proc.exitCode.then((code) {
        _append('Process exited: $code');
        _cleanupProcess();
      });
      setState(() {});
    } catch (e) {
      _append('Failed to start: $e');
    }
  }

  void _cleanupProcess() {
    _outSub?.cancel();
    _errSub?.cancel();
    _outSub = null;
    _errSub = null;
    _process = null;
    setState(() {});
  }

  Future<void> _stopProcess({bool force = false}) async {
    if (_process == null) return;
    try {
      _append('Stopping process...');
      if (force) {
        _process!.kill(ProcessSignal.sigkill);
      } else {
        _process!.kill();
      }
      await _process!.exitCode;
    } catch (e) {
      _append('Error stopping process: $e');
    } finally {
      _cleanupProcess();
    }
  }

  Future<void> _restartProcess() async {
    await _stopProcess();
    await Future.delayed(const Duration(milliseconds: 200));
    await _startProcess();
  }

  @override
  void dispose() {
    _cleanupProcess();
    _bin.dispose();
    _args.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final running = _process != null;
    return Scaffold(
      appBar: AppBar(title: const Text('Server Manager')),
      body: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(children: [
          Card(
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Column(children: [
                TextField(controller: _bin, decoration: const InputDecoration(labelText: 'Server binary path')),
                TextField(controller: _args, decoration: const InputDecoration(labelText: 'Arguments (space-separated)')),
                const SizedBox(height: 8),
                Row(children: [
                  hoverSfx(ElevatedButton(onPressed: running ? null : _startProcess, child: const Text('Start'))),
                  const SizedBox(width: 8),
                  hoverSfx(ElevatedButton(onPressed: running ? () => _stopProcess(force: false) : null, child: const Text('Stop'))),
                  const SizedBox(width: 8),
                  hoverSfx(ElevatedButton(onPressed: running ? _restartProcess : null, child: const Text('Restart'))),
                ])
              ]),
            ),
          ),
          const SizedBox(height: 8),
          Expanded(
            child: Card(
              child: Padding(
                padding: const EdgeInsets.all(8),
                child: SingleChildScrollView(
                  reverse: true,
                  child: Text(_logs.join('\n')),
                ),
              ),
            ),
          ),
        ]),
      ),
    );
  }
}
