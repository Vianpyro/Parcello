/// The menu grid's measurements, shared by the cards that must line up in
/// it. One home, because a tile and the private-table card agreeing on the
/// same numbers IS the layout.
library;

// Menu grid geometry. The small tiles are fixed cards; the private-table card
// spans exactly two of them plus the gap, and its *collapsed* body is pinned to
// one tile height so the row lines up. The header flexes to absorb whatever the
// footer leaves rather than being computed: a button's real height depends on
// Material's tap target and the platform's visual density, so arithmetic here
// would be wrong on some platform. Expanding a sub-action grows the card past
// the pinned body.
const double menuGap = 16;
const double menuTileW = 200;
const double menuTileH = 150;
const double footerBtnMinH = 44;
