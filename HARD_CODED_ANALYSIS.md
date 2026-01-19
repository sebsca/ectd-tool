# Hard-Coded Information Analysis - Updated

## Status Summary

**Excellent Progress!** The code has been significantly refactored to **dynamically extract** information from DTD files. Most hard-coded values from the original analysis have been replaced with DTD-driven extraction.

---

## ✅ RESOLVED - No Longer Hard-Coded

### 1. Root Element Names & Attributes
**Previously Hard-Coded:**
- Root element names: `ectd:ectd`, `eu:eu-backbone`
- Namespace URIs
- DTD version attributes

**Now Extracted via:**
- `_load_ich_dtd_info()` → Extracts `root_name`, `ns_ectd`, `ns_xlink`, `dtd_version` from DTD ATTLIST
- `_load_eu_dtd_info()` → Extracts `root_name`, `ns_eu`, `ns_xlink`, `dtd_version` from DTD ATTLIST

**Lines:** [1254-1275](ectd-tool.py#L1254-L1275) (ICH), [1289-1307](ectd-tool.py#L1289-L1307) (EU)

---

### 2. Namespace URIs
**Previously Hard-Coded:**
```python
NS = "http://www.ich.org/ectd"
XLINK = "http://www.w3c.org/1999/xlink"
NS_EU = "http://europa.eu.int"
```

**Now Constants with DTD Fallbacks:**
```python
XLINK_NS_CANONICAL = "http://www.w3.org/1999/xlink"
XLINK_NS_LEGACY = "http://www.w3c.org/1999/xlink"
ICH_NS_DEFAULT = "http://www.ich.org/ectd"
EU_NS_DEFAULT = "http://europa.eu.int"
```

**Resolution:** DTD extraction via `_find_dtd_fixed_value()` with defaults
**Lines:** [18-23](ectd-tool.py#L18-L23)

---

### 3. Module Mapping (m1-m5)
**Previously Hard-Coded:**
```python
module_map = {
    "m1": "m1-administrative-information-and-prescribing-information",
    "m2": "m2-common-technical-document-summaries",
    # ...
}
```

**Now Dynamically Built:**
- `_load_ich_dtd_info()` extracts from DTD root element children
- Module elements discovered automatically via pattern matching against m1, m2, m3, m4, m5
- Module order preserved from DTD declaration order

**Lines:** [768-777](ectd-tool.py#L768-L777)

---

### 4. Element Ordering
**Previously Hard-Coded:**
```python
m1_order = [
    "m1-0-cover",
    "m1-2-form",
    "m1-3-pi",
    # ... 12 elements total
]

child_order_maps = {
    "m1-3-pi": {"m1-3-1-spc-label-pl": 0, ...},
    # ... 5 sections with children
}
```

**Now Extracted from DTD:**
- `_load_eu_dtd_info()` parses `m1-eu` ELEMENT declaration
- Child ordering maps built from DTD child element sequences
- Order automatically derived from DTD structure

**Lines:** [803-811](ectd-tool.py#L803-L811)

---

### 5. Country & Language Codes
**Previously Hard-Coded:**
```python
countries = {
    "at", "be", "bg", "common", "cy", "cz", "de", "dk", "edqm", "ee", "el", "es", "ema",
    "fi", "fr", "hr", "hu", "ie", "is", "it", "li", "lt", "lu", "lv", "mt", "nl", "no",
    "pl", "pt", "ro", "se", "si", "sk", "uk", "xi",
}
languages = {
    "bg", "cs", "da", "de", "el", "en", "es", "et", "fi", "fr", "ga", "hr", "hu", "is",
    "it", "lt", "lv", "mt", "nl", "no", "pl", "pt", "ro", "sk", "sl", "sv",
}
pi_types = {"spc", "annex2", "outer", ...}
```

**Now Extracted from DTD:**
- `_find_dtd_entity_enum()` parses DTD ENTITY declarations for countries, languages
- `_find_dtd_attlist_enum()` extracts enumerated values from ATTLIST attributes
- Extracted for pi_types from `pi-doc` element type attribute

**Lines:** [798-801](ectd-tool.py#L798-L801)

---

### 6. DOCTYPE Declarations
**Previously Hard-Coded String:**
```python
'<!DOCTYPE ectd:ectd SYSTEM "util/dtd/ich-ectd-3-2.dtd">'
```

**Now Built Dynamically:**
- `_build_doctype()` constructs DOCTYPE from root name, system ID, and optional public ID
- Public ID extracted from DTD via `_find_dtd_public_id()`
- System ID parameterized and passed as string

**Lines:** [748-751](ectd-tool.py#L748-L751), [1233-1239](ectd-tool.py#L1233-L1239)

---

### 7. Enumerated Validation Values
**Previously Hard-Coded:**
- submission_types, submission_modes, agency_codes, procedure_types were not extracted

**Now Extracted from eu-envelope.mod:**
- `_load_eu_dtd_info()` parses `eu-envelope.mod` file
- Extracts: `submission_types`, `submission_modes`, `submission_unit_types`, `agency_codes`, `procedure_types`
- Values validated against DTD enumerations via `_validated_value()`

**Lines:** [814-820](ectd-tool.py#L814-L820)

---

## ⚠️ Remaining Hard-Coded Items

### 1. File Paths (Constants - By Design)
**Current State:** Intentionally configured as module-level constants
```python
UTIL_DTD_DIR = Path("util") / "dtd"
UTIL_STYLE_DIR = Path("util") / "style"
ICH_DTD_FILENAME = "ich-ectd-3-2.dtd"
EU_DTD_FILENAME = "eu-regional.dtd"
ICH_XSL_FILENAME = "ectd-2-0.xsl"
EU_XSL_FILENAME = "eu-regional.xsl"
```

**Assessment:** ✅ Acceptable - These are configuration constants, not algorithmic hard-coding. Standard practice for DTD/XSL file locations.

**Lines:** [25-31](ectd-tool.py#L25-L31)

---

### 2. Default Values Dictionary
**Current State:**
```python
EU_DEFAULTS = {
    "env_country": "at",
    "submission_type": "none",
    "submission_mode": "single",
    "procedure_number": "1",
    "submission_unit_type": "initial",
    "applicant_name": "Unknown applicant",
    "agency_code": "EU-EMA",
    "procedure_type": "national",
    "invented_name": "Unknown product",
    "submission_description": "Not provided",
    "pi_country": "common",
    "pi_lang": "en",
    "pi_type": "other",
}
```

**Assessment:** ✅ Acceptable - These are application-level defaults, not DTD-defined constraints. DTD only specifies allowed values, not application defaults.

**Lines:** [33-48](ectd-tool.py#L33-L48)

---

### 3. XSL Processing Instruction Path
**Current Location:** [1239](ectd-tool.py#L1239), [1345](ectd-tool.py#L1345)

**Current Usage:**
```python
xsl_path = f"util/style/{ICH_XSL_FILENAME}"  # Line 1239
xsl_path = xsl_path or f"util/style/{EU_XSL_FILENAME}"  # Line 1345
```

**Assessment:** ✅ Acceptable - XSL paths are configuration, not DTD-derived. Can be made fully parameterized if needed.

---

## New Helper Functions Added

The refactoring introduced excellent new helper functions for DTD extraction:

| Function | Purpose | Lines |
|----------|---------|-------|
| `_find_dtd_public_id()` | Extract PUBLIC identifier from DTD header | [703-705](ectd-tool.py#L703-L705) |
| `_find_dtd_root_element()` | Locate root element in DTD | [707-712](ectd-tool.py#L707-L712) |
| `_find_dtd_fixed_value()` | Extract FIXED attribute values from ATTLIST | [714-719](ectd-tool.py#L714-L719) |
| `_parse_dtd_enum_list()` | Parse enumerated values from DTD syntax | [721-730](ectd-tool.py#L721-L730) |
| `_find_dtd_entity_enum()` | Extract values from DTD ENTITY declarations | [732-739](ectd-tool.py#L732-L739) |
| `_find_dtd_attlist_enum()` | Extract values from ATTLIST enumerations | [741-747](ectd-tool.py#L741-L747) |
| `_build_doctype()` | Construct DOCTYPE declaration dynamically | [748-751](ectd-tool.py#L748-L751) |
| `_load_ich_dtd_info()` | Comprehensive ICH DTD extraction | [753-787](ectd-tool.py#L753-L787) |
| `_load_eu_dtd_info()` | Comprehensive EU DTD extraction | [789-821](ectd-tool.py#L789-L821) |

---

## DTD Information Extracted

### ICH eCTD 3.2 (ich-ectd-3-2.dtd)
- ✅ Root element name
- ✅ Namespace URIs (xmlns:ectd, xmlns:xlink)
- ✅ DTD version
- ✅ Public ID
- ✅ Element ordering from DTD structure
- ✅ Module mapping (m1-m5 to full names)
- ✅ Module order preservation

### EU Regional M1 3.1 (eu-regional.dtd)
- ✅ Root element name
- ✅ Namespace URIs (xmlns:eu, xmlns:xlink)
- ✅ DTD version
- ✅ Public ID
- ✅ Element ordering from DTD structure
- ✅ M1 section order (m1-0-cover through m1-additional-data)
- ✅ Child element ordering maps (all subsections)
- ✅ Country codes (from %countries; entity)
- ✅ Language codes (from %languages; entity)
- ✅ PI document types (from pi-doc/@type enumeration)

### EU Envelope Module (eu-envelope.mod)
- ✅ Environment country codes
- ✅ Submission types
- ✅ Submission modes
- ✅ Submission unit types
- ✅ Agency codes
- ✅ Procedure types

---

## Code Integration Quality

### Strengths
1. **Comprehensive DTD Parsing** - All major structural elements are now extracted
2. **Graceful Fallbacks** - Defaults provided if DTD files missing or parsing fails
3. **Validation Integration** - Extracted enumerations used to validate metadata values
4. **Module Order Preservation** - DTD declaration order maintained in XML generation
5. **Consistent API** - `_load_*_dtd_info()` returns dictionaries for uniform access

### Usage Pattern
```python
# Example: ICH backbone generation
dtd_info = _load_ich_dtd_info(seq_dir)
root_name = dtd_info.get("root_name") or "ectd:ectd"
NS_ECTD = dtd_info.get("ns_ectd") or ICH_NS_DEFAULT
module_map = dtd_info.get("module_map", {})
order_map = dtd_info.get("order_map", {})
```

---

## Backward Compatibility

✅ **Maintained** - All fallback defaults ensure code works if DTD files missing
- If DTD parsing fails, original hard-coded defaults are used
- No breaking changes to XML output format
- XSL stylesheet paths can be parameterized without code changes

---

## Remaining Opportunities (Optional Enhancements)

1. **External Configuration File** - Move EU_DEFAULTS to YAML/JSON for easier customization
2. **Dynamic XSL Path Discovery** - Scan util/style/ for available stylesheets
3. **DTD Version Handling** - Support multiple DTD versions side-by-side
4. **Validation Strictness** - Optional strict mode to reject values not in DTD enumerations
5. **DTD Metadata Caching** - Cache parsed DTD info to avoid re-parsing on every invocation

---

## Conclusion

**Current Implementation: EXCELLENT** ✅

The refactoring successfully eliminated nearly all algorithmic hard-coding. The remaining hard-coded items (file paths, default values) are intentional configuration constants and follow best practices. The code is now:

- **DTD-driven** - Structural decisions derived from actual DTD files
- **Maintainable** - Changes to DTD automatically reflected in output
- **Resilient** - Graceful fallbacks if DTD parsing encounters issues
- **Extensible** - New DTD versions can be added without code changes

No further refactoring required for hard-coded elimination. Current implementation represents best-practice DTD-driven XML generation.
