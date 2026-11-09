{
  "general": { "model": "TILT-HEAD 5-AXIS", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "crl", "name": "X", "type": "linear", "travel": { "min": -600.0, "max": 600.0 }, "parts": [
          { "id": "sld", "name": "Y", "type": "linear", "travel": { "min": -500.0, "max": 500.0 } }
        ] },
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -500.0, "max": 500.0 }, "safe_retract": 0.0, "parts": [
          { "id": "tla", "name": "A", "type": "rotary", "axis": [1.0, 0.0, 0.0],
            "travel": { "min": -120.0, "max": 120.0 }, "resolution": 0.001, "parts": [
            { "id": "tlb", "name": "B", "type": "rotary", "axis": [0.0, 1.0, 0.0],
              "travel": { "min": -120.0, "max": 120.0 }, "resolution": 0.001, "parts": [
              { "id": "hed", "name": "head", "type": "head",
                "spindle": { "min_speed": 50.0, "max_speed": 15000.0, "power": 15.0,
                             "body_diameter": 90.0, "body_length": 110.0 },
                "tool_station": { "max_tool_length": 300.0, "max_tool_diameter": 100.0,
                  "coolants": [
                    { "name": "Flood", "mCode": "M8" },
                    { "name": "Mist", "mCode": "M7" }
                  ] } }
            ] }
          ] }
        ] }
      ] }
    ] } },
  "controller": { "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 5000.0, "max_rapid_speed": 6000.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 5000.0, "max_rapid_speed": 6000.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 3000.0, "max_rapid_speed": 4000.0 },
    "A": { "coordinate": 3, "max_normal_speed": 2000.0, "max_rapid_speed": 5000.0 },
    "B": { "coordinate": 4, "max_normal_speed": 2000.0, "max_rapid_speed": 5000.0 }
  } } },
  "tooling": { "number_of_tools": 24 }
}
