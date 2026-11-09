{
  "general": { "model": "Tormach PCNC 1100 (Series 3)", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "inch", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -16.5, "max": 16.5 }, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -9.5, "max": 9.5 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -18.0, "max": 18.0 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "min_speed": 100.0, "max_speed": 5140.0, "power": 1.12,
            "ranges": [{ "min_speed": 100.0, "max_speed": 2000.0 }, { "min_speed": 250.0, "max_speed": 5140.0 }],
            "rigid_tapping": false },
          "tool_station": {
            "coolants": [
              { "name": "Flood", "mCode": "M8" },
              { "name": "Mist", "mCode": "M7" }
            ] } }
      ] }
    ] } },
  "controller": { "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 110.0, "max_rapid_speed": 110.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 110.0, "max_rapid_speed": 110.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 90.0, "max_rapid_speed": 90.0 }
  } } }
}
