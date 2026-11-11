{
  "name": "Doosan DNM 5700",
  "general": {
    "model": "Doosan DNM 5700",
    "vendor": "DN Solutions",
    "kind": "mill",
    "description": "DN Solutions DNM 5700 VMC (Fanuc 0i-MF). Specs: DN Solutions DNM 4500/5700/6700 Catalog (X/Y/Z: 1020/570/510mm, rapid 36/36/30 m/min, 12000 RPM spindle 18.5kW, rigid tapping, 30-tool ATC, optional 4th-axis rotary table)"
  },
  "capabilities": { "milling": true, "toolChange": true },
  "safeRetractZ": 0.0,
  "kinematics": {
    "static": {
      "head": {
        "spindle": { "min": 40.0, "max": 12000.0, "power": 18.5, "rigidTapping": true },
        "toolStation": {
          "maxToolLength": 300.0,
          "maxToolDiameter": 80.0,
          "coolants": [
            { "name": "Flood", "mCode": "M8" },
            { "name": "Mist", "mCode": "M7" },
            { "name": "Through-tool", "mCode": "M88" }
          ]
        }
      },
      "axes": [
        { "letter": "X", "direction": [1.0, 0.0, 0.0], "reversal": false,
          "speeds": { "rapid": 36000.0, "normal": 15000.0 }, "zeroOffset": 0.0, "tcp": 0.0 },
        { "letter": "Y", "direction": [0.0, 1.0, 0.0], "reversal": false,
          "speeds": { "rapid": 36000.0, "normal": 15000.0 }, "zeroOffset": 0.0, "tcp": 0.0 },
        { "letter": "Z", "direction": [0.0, 0.0, 1.0], "reversal": false,
          "speeds": { "rapid": 30000.0, "normal": 15000.0 }, "zeroOffset": 0.0, "tcp": 0.0 },
        { "letter": "A", "direction": [1.0, 0.0, 0.0], "axis": [1.0, 0.0, 0.0],
          "speeds": { "rapid": 12000.0, "normal": 7200.0 } }
      ],
      "rotaries": [
        { "letter": "A", "type": "rotary", "direction": [1.0, 0.0, 0.0], "travel": [-360000.0, 360000.0] }
      ],
      "table": { "rotation": "none" }
    }
  },
  "tooling": { "number_of_tools": 30 }
}
