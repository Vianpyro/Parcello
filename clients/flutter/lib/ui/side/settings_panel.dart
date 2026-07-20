/// The host's per-room settings editor (ADR-0015), read-only for guests.
library;

import 'package:flutter/material.dart';

import '../../l10n/app_localizations.dart';
import '../../protocol.dart';
import '../../session.dart';
import '../../tokens.dart';
import '../../typography.dart';
import '../common.dart';

class SettingsPanel extends StatefulWidget {
  final GameSession s;
  const SettingsPanel({super.key, required this.s});

  @override
  State<SettingsPanel> createState() => SettingsPanelState();
}

class SettingsPanelState extends State<SettingsPanel> {
  // Field keys in display order; labels are resolved per-locale in _hostLabel.
  static const _fieldKeys = [
    'game',
    'turn',
    'bank',
    'starting_balance',
    'go_salary',
    'velocity_min',
    'velocity_max',
    'max_houses',
    'bankruptcy_threshold',
    'expropriation',
    'rent_boost',
    'win_full_groups',
    'win_points',
    'subsidiary_pool',
    'conglomerate_pool',
    'spotlight_rent_pct',
    'spotlight_duration',
  ];
  late final Map<String, TextEditingController> _c;

  String _hostLabel(AppLocalizations t, String key) => switch (key) {
        'game' => t.settingGameLength,
        'turn' => t.settingTurnLimit,
        'bank' => t.settingTimeBank,
        'starting_balance' => t.settingStartingBalance,
        'go_salary' => t.settingGoSalary,
        'velocity_min' => t.settingVelocityMin,
        'velocity_max' => t.settingVelocityMax,
        'max_houses' => t.settingMaxHouses,
        'bankruptcy_threshold' => t.settingBankruptcyThreshold,
        'expropriation' => t.settingExpropriationPct,
        'rent_boost' => t.settingRentBoostPct,
        'win_full_groups' => t.settingDominationGroups,
        'win_points' => t.settingVictoryPointsTarget,
        'subsidiary_pool' => t.settingSubsidiaryPool,
        'conglomerate_pool' => t.settingConglomeratePool,
        'spotlight_rent_pct' => t.settingSpotlightRentPct,
        'spotlight_duration' => t.settingSpotlightDuration,
        _ => key,
      };

  @override
  void initState() {
    super.initState();
    final s = widget.s.settings!;
    final r = s.rules;
    int mins(int? secs) => secs == null ? 0 : secs ~/ 60;
    _c = {
      'game': TextEditingController(text: '${mins(s.gameSeconds)}'),
      'turn': TextEditingController(text: '${s.turnSeconds ?? 0}'),
      'bank': TextEditingController(text: '${s.timeBankSeconds ?? 0}'),
      'starting_balance': TextEditingController(text: '${r.startingBalance}'),
      'go_salary': TextEditingController(text: '${r.goSalary}'),
      'velocity_min': TextEditingController(text: '${r.velocityMin}'),
      'velocity_max': TextEditingController(text: '${r.velocityMax}'),
      'max_houses': TextEditingController(text: '${r.maxHousesPerProperty}'),
      'bankruptcy_threshold':
          TextEditingController(text: '${r.bankruptcyThreshold}'),
      'expropriation': TextEditingController(text: '${r.expropriation}'),
      'rent_boost': TextEditingController(text: '${r.rentBoost}'),
      'win_full_groups': TextEditingController(text: '${r.winFullGroups}'),
      'win_points': TextEditingController(text: '${r.winVictoryPoints}'),
      'subsidiary_pool':
          TextEditingController(text: '${r.subsidiaryPoolFactor}'),
      'conglomerate_pool':
          TextEditingController(text: '${r.conglomeratePoolFactor}'),
      'spotlight_rent_pct':
          TextEditingController(text: '${r.spotlightRentPct}'),
      'spotlight_duration':
          TextEditingController(text: '${r.spotlightDurationTurns}'),
    };
  }

  @override
  void dispose() {
    for (final c in _c.values) {
      c.dispose();
    }
    super.dispose();
  }

  int _n(String k) => int.tryParse(_c[k]!.text.trim()) ?? 0;

  void _apply() {
    final gameMin = _n('game'), turnSec = _n('turn'), bankSec = _n('bank');
    widget.s.configure({
      'game_seconds': gameMin > 0 ? gameMin * 60 : null,
      'turn_seconds': turnSec > 0 ? turnSec : null,
      'time_bank_seconds': bankSec > 0 ? bankSec : null,
      'rules': {
        'starting_balance': _n('starting_balance'),
        'go_salary': _n('go_salary'),
        'velocity_min': _n('velocity_min'),
        'velocity_max': _n('velocity_max'),
        'max_houses_per_property': _n('max_houses'),
        'bankruptcy_threshold': _n('bankruptcy_threshold'),
        'expropriation': _n('expropriation'),
        'rent_boost': _n('rent_boost'),
        'win_full_groups': _n('win_full_groups'),
        'win_victory_points': _n('win_points'),
        'subsidiary_pool_factor': _n('subsidiary_pool'),
        'conglomerate_pool_factor': _n('conglomerate_pool'),
        'spotlight_rent_pct': _n('spotlight_rent_pct'),
        'spotlight_duration_turns': _n('spotlight_duration'),
      },
    });
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.s.settings!;
    final t = AppLocalizations.of(context);
    final host = widget.s.seat == 0;
    return Theme(
      data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
      child: ExpansionTile(
        tilePadding: EdgeInsets.zero,
        childrenPadding: const EdgeInsets.only(bottom: Pc.s8),
        title: Text(t.settingsTitle,
            style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 14)),
        subtitle: Text(_summary(s, t),
            style: PcText.caption),
        children: host ? _hostFields(t) : _readOnly(s, t),
      ),
    );
  }

  String _summary(RoomSettings s, AppLocalizations t) {
    final g = s.gameSeconds == null
        ? t.settingOff
        : t.settingMinutes(s.gameSeconds! ~/ 60);
    final tn =
        s.turnSeconds == null ? t.settingOff : t.settingSeconds(s.turnSeconds!);
    final b = s.timeBankSeconds == null
        ? t.settingOff
        : t.settingSeconds(s.timeBankSeconds!);
    return t.settingsSummary(g, tn, b);
  }

  List<Widget> _hostFields(AppLocalizations t) => [
        for (final key in _fieldKeys)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 3),
            child: Row(children: [
              Expanded(
                  child: Text(_hostLabel(t, key),
                      style: const TextStyle(fontSize: 12))),
              SizedBox(
                width: 84,
                child: TextField(
                  controller: _c[key],
                  keyboardType: TextInputType.number,
                  textAlign: TextAlign.right,
                  decoration: const InputDecoration(isDense: true),
                ),
              ),
            ]),
          ),
        const SizedBox(height: Pc.s4),
        wideButton(t.settingApply, _apply, primary: false),
      ];

  List<Widget> _readOnly(RoomSettings s, AppLocalizations t) {
    final r = s.rules;
    final rows = <(String, String)>[
      (
        t.settingRoGameLength,
        s.gameSeconds == null
            ? t.settingOff
            : t.settingMinutes(s.gameSeconds! ~/ 60)
      ),
      (
        t.settingRoTurnLimit,
        s.turnSeconds == null ? t.settingOff : t.settingSeconds(s.turnSeconds!)
      ),
      (
        t.settingRoTimeBank,
        s.timeBankSeconds == null
            ? t.settingOff
            : t.settingSeconds(s.timeBankSeconds!)
      ),
      (t.settingStartingBalance, '\$${r.startingBalance}'),
      (t.settingGoSalary, '\$${r.goSalary}'),
      (t.settingRoVelocity, '${r.velocityMin}-${r.velocityMax}'),
      (t.settingRoMaxHouses, '${r.maxHousesPerProperty}'),
      (t.settingBankruptcyThreshold, '\$${r.bankruptcyThreshold}'),
      (
        t.settingRoExpropriation,
        r.expropriation == 0 ? t.settingOff : t.settingPercent(r.expropriation)
      ),
      (
        t.settingRoRentBoost,
        r.rentBoost == 0 ? t.settingOff : t.settingPercent(r.rentBoost)
      ),
      (
        t.settingRoDomination,
        r.winFullGroups == 0 ? t.settingOff : t.settingGroups(r.winFullGroups)
      ),
      (
        t.settingRoVictoryPoints,
        r.winVictoryPoints == 0 ? t.settingOff : '${r.winVictoryPoints}'
      ),
      (
        t.settingRoSubsidiaryPool,
        r.subsidiaryPoolFactor == 0
            ? t.settingOff
            : t.settingPoolFactor(r.subsidiaryPoolFactor)
      ),
      (
        t.settingRoConglomeratePool,
        r.conglomeratePoolFactor == 0
            ? t.settingOff
            : t.settingPoolFactor(r.conglomeratePoolFactor)
      ),
      (
        t.settingRoSpotlight,
        r.spotlightRentPct == 0
            ? t.settingOff
            : t.settingSpotlightValue(
                r.spotlightRentPct, r.spotlightDurationTurns)
      ),
    ];
    return [
      for (final (label, value) in rows)
        Padding(
          padding: const EdgeInsets.symmetric(vertical: Pc.s2),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(label, style: const TextStyle(fontSize: 12)),
              Text(value,
                  style: const TextStyle(
                      fontSize: 12, fontWeight: FontWeight.w600)),
            ],
          ),
        ),
    ];
  }
}

/// Post-game survey card (side panel, dismissible, one per game).
