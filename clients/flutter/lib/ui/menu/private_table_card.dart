/// The private-table card: create, create modded, or join - inline.
library;

import 'package:flutter/material.dart';
import '../../l10n/app_localizations.dart';
import '../../motion.dart';
import '../../session.dart';
import '../../sfx.dart';
import '../../tokens.dart';
import '../../typography.dart';
import '../common.dart';
import 'geometry.dart';

/// Which sub-action the private-table card has expanded inline, if any.
enum _TableAction { none, modded, join }

/// The private-table card: one Business-Tour-style card whose split footer
/// carries the three room actions. Create is a single tap (server-default
/// mods - the common case must stay one click); Modded and Join expand
/// *inside* the card, no modal. The mod picker is fed by the server's
/// `list_mods` answer so nobody ever types a mod id; picking order is kept
/// because later mods override earlier ones (ADR-0006) - same tap-to-order
/// chips as the Legal Route builder.
class PrivateTableCard extends StatefulWidget {
  final GameSession s;
  const PrivateTableCard({super.key, required this.s});

  @override
  State<PrivateTableCard> createState() => PrivateTableCardState();
}

class PrivateTableCardState extends State<PrivateTableCard> {
  final _code = TextEditingController();
  _TableAction _open = _TableAction.none;
  /// Picked mod ids, in pick order (the order sent on the wire).
  final List<String> _picked = [];

  @override
  void dispose() {
    _code.dispose();
    super.dispose();
  }

  void _toggle(_TableAction a) {
    setState(() => _open = _open == a ? _TableAction.none : a);
    // Lazy: ask for the list only when the picker opens without an answer.
    if (_open == _TableAction.modded && widget.s.availableMods == null) {
      widget.s.requestMods();
    }
  }

  void _join() {
    if (_code.text.trim().isNotEmpty) widget.s.joinGame(_code.text);
  }

  Widget _footerBtn(String label,
      {required VoidCallback onTap,
      bool selected = false,
      bool autofocus = false}) {
    return Expanded(
      child: hoverSfx(TextButton(
        autofocus: autofocus,
        onPressed: onTap,
        style: TextButton.styleFrom(
          foregroundColor: selected ? Pc.gold : Pc.text,
          backgroundColor:
              selected ? Pc.gold.withValues(alpha: 0.12) : null,
          shape: const RoundedRectangleBorder(borderRadius: Pc.radius),
          minimumSize: const Size(0, footerBtnMinH),
        ),
        child: Text(label,
            style: const TextStyle(fontSize: 14, fontWeight: FontWeight.w700)),
      )),
    );
  }

  Widget _hairline() => Container(width: 1, height: Pc.s24, color: Pc.border);

  /// One selectable mod chip; picked chips show their play position, tapping
  /// again removes them (mirrors `_routeChip` on the board).
  Widget _modChip(String id) {
    final pos = _picked.indexOf(id);
    final picked = pos >= 0;
    return hoverSfx(OutlinedButton(
      onPressed: () => setState(() {
        if (picked) {
          _picked.remove(id);
        } else {
          _picked.add(id);
        }
      }),
      style: OutlinedButton.styleFrom(
        shape: const RoundedRectangleBorder(borderRadius: Pc.radius),
        backgroundColor: picked ? Pc.gold.withValues(alpha: 0.3) : null,
        side: BorderSide(color: picked ? Pc.goldDark : Pc.textMuted),
        minimumSize: const Size(0, 40),
      ),
      child: Text(picked ? '$id  #${pos + 1}' : id),
    ));
  }

  Widget _modPicker(AppLocalizations t) {
    final mods = widget.s.availableMods;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (mods == null)
        Text(t.modsLoading,
            style: PcText.label.copyWith(color: Pc.textMuted))
      else if (mods.isEmpty)
        Text(t.modsUnavailable,
            style: PcText.label.copyWith(color: Pc.textMuted))
      else ...[
        Text(t.modsOrderHint,
            style: PcText.caption.copyWith(color: Pc.textFaint)),
        const SizedBox(height: Pc.s6),
        Wrap(spacing: 6, runSpacing: 6, children: [
          for (final id in mods) _modChip(id),
        ]),
      ],
      const SizedBox(height: 10),
      // Empty selection sends no `mods` field at all - the server default.
      wideButton(t.create,
          () => widget.s.createGame(mods: List.of(_picked))),
    ]);
  }

  Widget _joinPanel(AppLocalizations t) {
    return Row(children: [
      Expanded(
        child: TextField(
          controller: _code,
          autofocus: true,
          maxLength: 5,
          textCapitalization: TextCapitalization.characters,
          onSubmitted: (_) => _join(),
          decoration: InputDecoration(
              labelText: t.roomCode, counterText: '', isDense: true),
        ),
      ),
      const SizedBox(width: Pc.s8),
      hoverSfx(FilledButton(onPressed: _join, child: Text(t.join))),
    ]);
  }

  @override
  Widget build(BuildContext context) {
    final t = AppLocalizations.of(context);
    return SizedBox(
      width: menuTileW * 2 + menuGap,
      child: Card(
        // Zero margin so the card's box is exactly the body below - a default
        // Card margin would push it past a tile and break the row.
        margin: EdgeInsets.zero,
        clipBehavior: Clip.antiAlias,
        color: Pc.surface,
        child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              SizedBox(
                height: menuTileH,
                child: Column(children: [
                  Expanded(
                    child: Padding(
                      padding: const EdgeInsets.all(Pc.s16),
                      child: Row(children: [
                        const Icon(Icons.casino_outlined,
                            size: 40, color: Pc.gold),
                        const SizedBox(width: Pc.s12),
                        Expanded(
                          child: Column(
                              mainAxisSize: MainAxisSize.min,
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(t.menuPrivateTitle,
                                    style: PcText.tileTitle),
                                const SizedBox(height: Pc.s4),
                                Text(t.menuPrivateSubtitle,
                                    style: PcText.body.copyWith(color: Pc.textMuted)),
                              ]),
                        ),
                      ]),
                    ),
                  ),
                  const Divider(height: 1, color: Pc.border),
                  Row(children: [
                    _footerBtn(t.create,
                        autofocus: true, onTap: () => widget.s.createGame()),
                    _hairline(),
                    _footerBtn(t.createModded,
                        selected: _open == _TableAction.modded,
                        onTap: () => _toggle(_TableAction.modded)),
                    _hairline(),
                    _footerBtn(t.join,
                        selected: _open == _TableAction.join,
                        onTap: () => _toggle(_TableAction.join)),
                  ]),
                ]),
              ),
              // The sub-action grows the card in place - never a modal.
              AnimatedSize(
                duration: Motion.establish,
                curve: Motion.deliberate,
                alignment: Alignment.topCenter,
                child: switch (_open) {
                  _TableAction.none => const SizedBox(width: double.infinity),
                  _TableAction.modded => Padding(
                      padding: const EdgeInsets.fromLTRB(12, 10, 12, 12),
                      child: _modPicker(t)),
                  _TableAction.join => Padding(
                      padding: const EdgeInsets.fromLTRB(12, 10, 12, 12),
                      child: _joinPanel(t)),
                },
              ),
            ]),
      ),
    );
  }
}
