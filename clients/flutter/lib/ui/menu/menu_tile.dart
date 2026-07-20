/// One tile of the main menu's card grid.
library;

import 'package:flutter/material.dart';
import '../../sfx.dart';
import '../../tokens.dart';
import '../../typography.dart';
import 'geometry.dart';

/// One large action card in the main menu. Stateful so it can paint a visible
/// focus ring: on a controller / Steam Deck the player navigates these with
/// the D-pad (arrow keys) and activates with A (Enter/Space, handled by the
/// InkWell), so which tile is selected must be unmistakable.
class MenuTile extends StatefulWidget {
  final IconData icon;
  final String title;
  final String subtitle;
  final VoidCallback onTap;
  const MenuTile({
    super.key,
    required this.icon,
    required this.title,
    required this.subtitle,
    required this.onTap,
  });

  @override
  State<MenuTile> createState() => MenuTileState();
}

class MenuTileState extends State<MenuTile> {
  bool _focused = false;

  @override
  Widget build(BuildContext context) {
    return hoverSfx(SizedBox(
      width: menuTileW,
      height: menuTileH,
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 120),
        decoration: BoxDecoration(
          borderRadius: Pc.radius,
          border: Border.all(
            color: _focused ? Pc.gold : Pc.border,
            width: _focused ? 2 : 1,
          ),
        ),
        child: Card(
          margin: EdgeInsets.zero,
          clipBehavior: Clip.antiAlias,
          color: Pc.surface,
          child: InkWell(
            onFocusChange: (f) => setState(() => _focused = f),
            focusColor: Pc.gold.withValues(alpha: 0.12),
            onTap: widget.onTap,
            child: Padding(
              padding: const EdgeInsets.all(Pc.s16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Icon(widget.icon, size: 40, color: Pc.gold),
                  const Spacer(),
                  // The tile is a fixed height, so the labels must be bounded:
                  // a longer translation (French runs longer than English) has
                  // to ellipsize, never overflow the card.
                  Flexible(
                    child: Text(widget.title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: PcText.tileTitle),
                  ),
                  const SizedBox(height: Pc.s4),
                  Flexible(
                    child: Text(widget.subtitle,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: PcText.body.copyWith(color: Pc.textMuted)),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    ));
  }
}
