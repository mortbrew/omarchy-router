import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

// OpenRouter spend at four horizons: 1h, today, 7d, 30d.
//
// One CLI call (`--brief`) returns all four, so the widget never fans out into
// four processes. "today" is local midnight to now, not a rolling 24 hours, so
// it resets when the day does.
//
// Left click opens a popup listing the four horizons; the ones you tick are the
// ones the bar shows. The choice is written back to shell.json, so it survives
// a restart. Right click refreshes immediately.
BarWidget {
  id: root
  moduleName: "mortbrew.openrouter-cost"

  readonly property string cliPath: String(setting("cliPath", "omarchy-router"))
  readonly property int intervalMinutes: Math.max(1, Number(setting("intervalMinutes", 15)))
  readonly property bool showLabels: setting("showLabels", true) !== false

  // label -> amount string, in display order.
  property var values: ({})
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

  // Which horizons the bar shows. Defaults to all four, matching the original
  // widget, and is persisted as a comma-separated `visible` setting.
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

  // The CLI prints "1h=0.1547 today=0.1948 7d=6.5389 30d=7.1537".
  function parse(text) {
    var out = {}
    var parts = String(text || "").trim().split(/\s+/)
    for (var i = 0; i < parts.length; i++) {
      var m = parts[i].match(/^([0-9a-z]+)=(-?[\d.]+)$/)
      if (m) out[m[1]] = m[2]
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
    command: [root.cliPath, "--brief"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var parsed = root.parse(text)
        if (parsed["today"] !== undefined) {
          root.values = parsed
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
          text: root.failed ? "—" : root.money(root.values[modelData])
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
        : "OpenRouter spend — click to choose which horizons show.")
    }
    onExited: if (root.bar) root.bar.hideTooltip(root)
  }

  PopupCard {
    id: popup
    anchorItem: root
    bar: root.bar
    owner: root
    open: root.popupOpen
    contentWidth: popup.fittedContentWidth(Style.space(300))
    contentHeight: popup.fittedContentHeight(column.implicitHeight)

    Column {
      id: column
      anchors.fill: parent
      spacing: Style.space(8)

      Text {
        textFormat: Text.PlainText
        text: "Show in the bar"
        color: root.bar ? root.bar.foreground : Commons.Color.foreground
        font.family: root.bar ? root.bar.fontFamily : Style.font.family
        font.pixelSize: Style.font.subtitle
        font.bold: true
        width: parent.width
      }

      Repeater {
        model: root.order

        Toggle {
          required property string modelData

          width: column.width
          label: root.titles[modelData] || modelData
          description: root.descriptions[modelData] || ""
          checked: root.isShown(modelData)
          foreground: root.bar ? root.bar.foreground : Commons.Color.foreground
          accent: Commons.Color.accent
          fontFamily: root.bar ? root.bar.fontFamily : Style.font.family
          // The last remaining horizon cannot be switched off, or the widget
          // would render nothing and the popup would be unreachable.
          enabled: !(root.isShown(modelData) && root.shown.length === 1)
          opacity: enabled ? 1.0 : 0.5
          onClicked: root.toggleShown(modelData)
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
          text: root.failed ? "No data — check the management key." : "Updated every " + root.intervalMinutes + " min."
          color: Qt.darker(root.bar ? root.bar.foreground : Commons.Color.foreground, 1.5)
          font.family: root.bar ? root.bar.fontFamily : Style.font.family
          font.pixelSize: Style.font.caption
          elide: Text.ElideRight
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
