import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// OpenRouter spend at four horizons: 1h, today, 7d, 30d.
//
// One CLI call (`--stats`) returns every horizon with its full stat set, so the
// widget never fans out into four processes. "today" is local midnight to now,
// not a rolling 24 hours, so it resets when the day does.
//
// Left click opens a popup with the full breakdown per horizon; the horizons you
// tick are the ones the bar shows. The choice is written back to shell.json, so
// it survives a restart. Right click refreshes immediately.
BarWidget {
  id: root
  moduleName: "mortbrew.openrouter-cost"

  readonly property string cliPath: String(setting("cliPath", "omarchy-router"))
  readonly property int intervalMinutes: Math.max(1, Number(setting("intervalMinutes", 15)))
  readonly property bool showLabels: setting("showLabels", true) !== false
  readonly property bool showIcon: setting("showIcon", true) !== false

  // DeepSeek's mark, from Omarchy's own icon font (U+E911). The widget is
  // built for a DeepSeek-only setup, so the mark says which model the money
  // went to without spending bar width on a word.
  readonly property string iconGlyph: "\ue911"
  readonly property string iconFamily: "omarchy"

  // label -> { cost, requests, tokens, cacheHit, blended }, in display order.
  property var stats: ({})
  property bool failed: false
  property bool popupOpen: false

  // Every horizon the CLI reports, in the order they are offered.
  readonly property var order: ["1h", "today", "7d", "30d"]

  // Human wording for the popup rows.
  readonly property var titles: ({
    "1h": "Last hour",
    "today": "Today",
    "7d": "Last 7 days",
    "30d": "Last 30 days"
  })

  readonly property var descriptions: ({
    "1h": "Rolling 60 minutes",
    "today": "From local midnight",
    "7d": "Rolling 7 days",
    "30d": "Rolling 30 days"
  })

  // Which horizons the bar shows. Defaults to all four, and is persisted as a
  // comma-separated `visible` setting.
  readonly property var shown: {
    var raw = setting("visible", "")
    if (raw === "" || raw === null || raw === undefined) return root.order.slice()
    var picked = String(raw).split(",").map(function(s) { return s.trim() })
    var out = []
    for (var i = 0; i < root.order.length; i++) {
      if (picked.indexOf(root.order[i]) !== -1) out.push(root.order[i])
    }
    // An empty or unrecognised setting would leave a blank widget; fall back.
    return out.length > 0 ? out : root.order.slice()
  }

  function isShown(label) {
    return root.shown.indexOf(label) !== -1
  }

  function refresh() {
    if (!costProc.running) costProc.running = true
  }

  // The CLI prints one line per horizon:
  //   "1h 0.2015 355 24140285 0.9746 0.008345"
  // being label, cost, requests, tokens, cache hit rate, blended $/M.
  function parse(text) {
    var out = {}
    var lines = String(text || "").trim().split("\n")
    for (var i = 0; i < lines.length; i++) {
      var f = lines[i].trim().split(/\s+/)
      if (f.length < 6) continue
      if (root.order.indexOf(f[0]) === -1) continue
      out[f[0]] = {
        cost: Number(f[1]),
        requests: Number(f[2]),
        tokens: Number(f[3]),
        cacheHit: Number(f[4]),
        blended: Number(f[5])
      }
    }
    return out
  }

  // Compact money: cents matter below $10, whole dollars above it.
  function money(raw) {
    var v = Number(raw)
    if (!isFinite(v)) return "—"
    if (v === 0) return "$0"
    if (v < 0.01) return "<$0.01"
    if (v < 10) return "$" + v.toFixed(2)
    return "$" + Math.round(v)
  }

  // Full money for the popup, where there is room for cents.
  function moneyExact(raw) {
    var v = Number(raw)
    if (!isFinite(v)) return "—"
    if (v === 0) return "$0.00"
    if (v < 0.01) return "$" + v.toFixed(4)
    return "$" + v.toFixed(2)
  }

  // 24140285 -> "24.1M", 6842 -> "6.8K".
  function compact(raw) {
    var v = Number(raw)
    if (!isFinite(v)) return "—"
    if (v >= 1e9) return (v / 1e9).toFixed(1) + "B"
    if (v >= 1e6) return (v / 1e6).toFixed(1) + "M"
    if (v >= 1e3) return (v / 1e3).toFixed(1) + "K"
    return String(Math.round(v))
  }

  function percent(raw) {
    var v = Number(raw)
    if (!isFinite(v)) return "—"
    return (v * 100).toFixed(1) + "%"
  }

  function stat(label) {
    return root.stats[label] || null
  }

  // Persist the selection. Applied locally first so the bar updates on the
  // click itself; the shell.json write comes back through the bar as the same
  // value.
  function setShown(labels) {
    var ordered = []
    for (var i = 0; i < root.order.length; i++) {
      if (labels.indexOf(root.order[i]) !== -1) ordered.push(root.order[i])
    }
    // Never persist an empty selection: the widget would render nothing and
    // there would be no way back to the popup.
    if (ordered.length === 0) return

    var entry = { id: root.moduleName }
    for (var key in root.settings) if (key !== "id") entry[key] = root.settings[key]
    entry["visible"] = ordered.join(",")

    root.settings = entry
    if (root.bar && root.bar.shell && typeof root.bar.shell.updateEntryInline === "function")
      root.bar.shell.updateEntryInline(root.moduleName, entry)
  }

  function toggleShown(label) {
    var next = root.shown.slice()
    var at = next.indexOf(label)
    if (at === -1) next.push(label)
    else next.splice(at, 1)
    root.setShown(next)
  }

  function close() { popupOpen = false }

  visible: true
  implicitWidth: row.implicitWidth + Style.spacing.controlPaddingX * 2
  implicitHeight: barSize

  Process {
    id: costProc
    command: [root.cliPath, "--stats"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var parsed = root.parse(text)
        if (parsed["today"] !== undefined) {
          root.stats = parsed
          root.failed = false
        } else {
          root.failed = true
        }
      }
    }
    onExited: function(exitCode) {
      if (exitCode !== 0) root.failed = true
    }
  }

  Timer {
    interval: root.intervalMinutes * 60000
    running: true
    repeat: true
    triggeredOnStart: true
    onTriggered: root.refresh()
  }

  Row {
    id: row
    // Anchor to the parent's edges rather than centering: the bar sizes the
    // widget from implicitWidth, which reads row.implicitWidth, so centering
    // inside a parent sized by this row is circular and collapses to zero.
    anchors.verticalCenter: parent.verticalCenter
    anchors.left: parent.left
    anchors.leftMargin: Style.spacing.controlPaddingX
    spacing: Style.space(10)

    Text {
      visible: root.showIcon
      anchors.verticalCenter: parent.verticalCenter
      textFormat: Text.PlainText
      text: root.iconGlyph
      color: root.bar ? root.bar.barForeground : Commons.Color.foreground
      font.family: root.iconFamily
      font.pixelSize: Style.font.body
      opacity: 0.85
    }

    Repeater {
      model: root.shown

      Row {
        spacing: Style.space(3)
        anchors.verticalCenter: parent.verticalCenter

        Text {
          visible: root.showLabels
          anchors.verticalCenter: parent.verticalCenter
          textFormat: Text.PlainText
          text: modelData
          color: root.bar ? root.bar.barForeground : Commons.Color.foreground
          font.family: root.bar ? root.bar.fontFamily : Style.font.family
          font.pixelSize: Style.font.body
          opacity: 0.45
        }

        Text {
          anchors.verticalCenter: parent.verticalCenter
          textFormat: Text.PlainText
          text: root.failed ? "—" : root.money(root.stat(modelData) ? root.stat(modelData).cost : NaN)
          color: root.bar ? root.bar.barForeground : Commons.Color.foreground
          font.family: root.bar ? root.bar.fontFamily : Style.font.family
          font.pixelSize: Style.font.body
          opacity: root.failed ? 0.5 : 0.85
        }
      }
    }
  }

  MouseArea {
    anchors.fill: parent
    hoverEnabled: true
    acceptedButtons: Qt.LeftButton | Qt.RightButton
    cursorShape: Qt.PointingHandCursor
    onClicked: function(mouse) {
      if (mouse.button === Qt.RightButton) root.refresh()
      else root.popupOpen = !root.popupOpen
    }
    onEntered: if (root.bar) {
      root.bar.showTooltip(root, root.failed
        ? "OpenRouter: no data (click to choose, right-click to retry)"
        : "OpenRouter spend — click for the breakdown.")
    }
    onExited: if (root.bar) root.bar.hideTooltip(root)
  }

  PopupCard {
    id: popup
    anchorItem: root
    bar: root.bar
    owner: root
    open: root.popupOpen
    contentWidth: popup.fittedContentWidth(Style.space(340))
    contentHeight: popup.fittedContentHeight(column.implicitHeight)

    Column {
      id: column
      anchors.fill: parent
      spacing: Style.space(10)

      // ---- Header: the mark, and what the numbers are.
      Row {
        width: parent.width
        spacing: Style.space(8)

        Text {
          anchors.verticalCenter: parent.verticalCenter
          textFormat: Text.PlainText
          text: root.iconGlyph
          color: root.bar ? root.bar.foreground : Commons.Color.foreground
          font.family: root.iconFamily
          font.pixelSize: Style.font.subtitle
        }

        Text {
          anchors.verticalCenter: parent.verticalCenter
          textFormat: Text.PlainText
          text: "OpenRouter spend"
          color: root.bar ? root.bar.foreground : Commons.Color.foreground
          font.family: root.bar ? root.bar.fontFamily : Style.font.family
          font.pixelSize: Style.font.subtitle
          font.bold: true
        }
      }

      // ---- One card per horizon, each with the full stat set.
      Repeater {
        model: root.order

        BorderSurface {
          id: card
          required property string modelData

          readonly property var s: root.stat(modelData)
          readonly property bool on: root.isShown(modelData)

          width: column.width
          radius: Style.spacing.labelGap
          color: on ? Style.selectedFillFor(root.bar ? root.bar.foreground : Commons.Color.foreground, Commons.Color.accent) : "transparent"
          borderSpec: on
            ? Border.controlSpec("normal", root.bar ? root.bar.foreground : Commons.Color.foreground, Commons.Color.accent)
            : Border.none()
          implicitHeight: inner.implicitHeight + Style.space(16)

          Column {
            id: inner
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.leftMargin: card.borderLeft + Style.space(10)
            anchors.rightMargin: card.borderRight + Style.space(10)
            spacing: Style.space(6)

            // Title row: name, then the toggle state as a check.
            Row {
              width: parent.width
              spacing: Style.space(6)

              Text {
                textFormat: Text.PlainText
                text: root.titles[card.modelData] || card.modelData
                color: root.bar ? root.bar.foreground : Commons.Color.foreground
                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.bodySmall
                font.bold: true
                width: parent.width - check.width - parent.spacing
                elide: Text.ElideRight
              }

              Text {
                id: check
                textFormat: Text.PlainText
                text: card.on ? "Shown" : "Hidden"
                color: Qt.darker(root.bar ? root.bar.foreground : Commons.Color.foreground, card.on ? 1.2 : 1.7)
                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.caption
              }
            }

            // The stat grid: cost and requests on one line, tokens and cache
            // hit on the next, blended $/M last.
            Grid {
              width: parent.width
              columns: 2
              columnSpacing: Style.space(10)
              rowSpacing: Style.space(4)

              Column {
                width: (inner.width - Style.space(10)) / 2
                spacing: Style.space(1)
                Text {
                  textFormat: Text.PlainText
                  text: "Total spend"
                  color: Qt.darker(root.bar ? root.bar.foreground : Commons.Color.foreground, 1.6)
                  font.family: root.bar ? root.bar.fontFamily : Style.font.family
                  font.pixelSize: Style.font.caption
                }
                Text {
                  textFormat: Text.PlainText
                  text: card.s ? root.moneyExact(card.s.cost) : "—"
                  color: root.bar ? root.bar.foreground : Commons.Color.foreground
                  font.family: root.bar ? root.bar.fontFamily : Style.font.family
                  font.pixelSize: Style.font.subtitle
                  font.bold: true
                }
              }

              Column {
                width: (inner.width - Style.space(10)) / 2
                spacing: Style.space(1)
                Text {
                  textFormat: Text.PlainText
                  text: "Requests"
                  color: Qt.darker(root.bar ? root.bar.foreground : Commons.Color.foreground, 1.6)
                  font.family: root.bar ? root.bar.fontFamily : Style.font.family
                  font.pixelSize: Style.font.caption
                }
                Text {
                  textFormat: Text.PlainText
                  text: card.s ? root.compact(card.s.requests) : "—"
                  color: root.bar ? root.bar.foreground : Commons.Color.foreground
                  font.family: root.bar ? root.bar.fontFamily : Style.font.family
                  font.pixelSize: Style.font.subtitle
                  font.bold: true
                }
              }

              Column {
                width: (inner.width - Style.space(10)) / 2
                spacing: Style.space(1)
                Text {
                  textFormat: Text.PlainText
                  text: "Token volume"
                  color: Qt.darker(root.bar ? root.bar.foreground : Commons.Color.foreground, 1.6)
                  font.family: root.bar ? root.bar.fontFamily : Style.font.family
                  font.pixelSize: Style.font.caption
                }
                Text {
                  textFormat: Text.PlainText
                  text: card.s ? root.compact(card.s.tokens) : "—"
                  color: root.bar ? root.bar.foreground : Commons.Color.foreground
                  font.family: root.bar ? root.bar.fontFamily : Style.font.family
                  font.pixelSize: Style.font.subtitle
                  font.bold: true
                }
              }

              Column {
                width: (inner.width - Style.space(10)) / 2
                spacing: Style.space(1)
                Text {
                  textFormat: Text.PlainText
                  text: "Cache hit rate"
                  color: Qt.darker(root.bar ? root.bar.foreground : Commons.Color.foreground, 1.6)
                  font.family: root.bar ? root.bar.fontFamily : Style.font.family
                  font.pixelSize: Style.font.caption
                }
                Text {
                  textFormat: Text.PlainText
                  text: card.s ? root.percent(card.s.cacheHit) : "—"
                  color: root.bar ? root.bar.foreground : Commons.Color.foreground
                  font.family: root.bar ? root.bar.fontFamily : Style.font.family
                  font.pixelSize: Style.font.subtitle
                  font.bold: true
                }
              }
            }

            // Blended cost gets its own line: it is a rate, not a total.
            Row {
              width: parent.width
              spacing: Style.space(4)
              Text {
                textFormat: Text.PlainText
                text: "Blended"
                color: Qt.darker(root.bar ? root.bar.foreground : Commons.Color.foreground, 1.6)
                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.caption
              }
              Text {
                textFormat: Text.PlainText
                text: card.s ? root.moneyExact(card.s.blended) + " /M tokens" : "—"
                color: Qt.darker(root.bar ? root.bar.foreground : Commons.Color.foreground, 1.25)
                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.caption
              }
            }
          }

          MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            // The last remaining horizon cannot be switched off, or the widget
            // would render nothing and the popup would be unreachable.
            enabled: !(card.on && root.shown.length === 1)
            onClicked: root.toggleShown(card.modelData)
          }
        }
      }

      PanelSeparator {
        foreground: root.bar ? root.bar.foreground : Commons.Color.foreground
      }

      Row {
        width: parent.width
        spacing: Style.space(8)

        Text {
          textFormat: Text.PlainText
          anchors.verticalCenter: parent.verticalCenter
          width: parent.width - refreshButton.width - parent.spacing
          text: root.failed
            ? "No data — check the management key."
            : "Click a card to show or hide it. Updated every " + root.intervalMinutes + " min."
          color: Qt.darker(root.bar ? root.bar.foreground : Commons.Color.foreground, 1.5)
          font.family: root.bar ? root.bar.fontFamily : Style.font.family
          font.pixelSize: Style.font.caption
          wrapMode: Text.WordWrap
        }

        Button {
          id: refreshButton
          text: "Refresh"
          foreground: root.bar ? root.bar.foreground : Commons.Color.foreground
          horizontalPadding: Style.spacing.controlPaddingX
          verticalPadding: Style.spacing.controlPaddingY
          onClicked: root.refresh()
        }
      }
    }
  }
}
