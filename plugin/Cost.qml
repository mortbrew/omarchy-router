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
BarWidget {
  id: root
  moduleName: "mortbrew.openrouter-cost"

  readonly property string cliPath: String(setting("cliPath", "omarchy-router"))
  readonly property int intervalMinutes: Math.max(1, Number(setting("intervalMinutes", 15)))
  readonly property bool showLabels: setting("showLabels", true) !== false

  // label -> amount string, in display order.
  property var values: ({})
  property bool failed: false

  readonly property var order: ["1h", "today", "7d", "30d"]

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
      model: root.order

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
    cursorShape: Qt.PointingHandCursor
    onClicked: root.refresh()
    onEntered: if (root.bar) {
      root.bar.showTooltip(root, root.failed
        ? "OpenRouter: no data (click to retry)"
        : "OpenRouter spend — 1h, today (from midnight), 7d, 30d. Click to refresh.")
    }
    onExited: if (root.bar) root.bar.hideTooltip(root)
  }
}
