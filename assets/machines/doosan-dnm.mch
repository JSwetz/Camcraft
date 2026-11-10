{
  "name": "Doosan DNM 5700",
  "capabilities": { "milling": true, "toolChange": true },
  "safeRetractZ": 0.0,
  "kinematics": {
    "static": {
      "head": {
        "spindle": { "min": 0.0, "max": 12000.0 },
        "toolStation": {
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
          "speeds": { "rapid": 30000.0, "normal": 15000.0 }, "zeroOffset": 0.0, "tcp": 0.0 }
      ],
      "table": { "rotation": "none" }
    }
  }
}
