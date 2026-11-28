# Data sources

Data bundled with Housewright, where it comes from, its licence, and how to
rebuild it. Nothing here is scraped and nothing is fetched at run time.

## US ZIP code climate table

File: `crates/core/data/us-zip-climate.bin` (198,874 bytes; about 95 KB
gzipped). Read by `crates/core/src/climate.rs`, which documents the binary
format. Decision: D-64.

For each of the 33,144 ZIP Code Tabulation Areas (ZCTAs) it holds:

- the average **first autumn frost** (the date by which the first night at or
  below 32 °F has happened in half of years);
- the average **last spring frost** (the same, for the last such night);
- whether a **hard freeze** (28 °F) happens in at least half of years, used to
  decide whether winterising outdoor taps is worth suggesting;
- the hemisphere (only American Samoa is south).

Frost dates are stored as days of a non-leap year, as the schedule engine
expects (D-55).

### Sources

| Source | What we use | Licence |
|---|---|---|
| NOAA National Centers for Environmental Information (NCEI), *U.S. Climate Normals 1991-2020*, annual/seasonal normals by station, version 1.0.1 (archive `us-climate-normals_1991-2020_v1.0.1_annualseasonal_multivariate_by-station_c20230404.tar.gz`, from `https://www.ncei.noaa.gov/data/normals-annualseasonal/1991-2020/archive/`) | per station: latitude, longitude, `ANN-TMIN-PRBFST-T32FP50`, `ANN-TMIN-PRBLST-T32FP50`, `ANN-TMIN-PRBFST-T28FP50`, `ANN-TMIN-AVGNDS-LSTH032` and their measurement flags | Work of the US federal government: public domain in the United States (17 U.S.C. §105). NOAA asks that the data be cited; see the citation below. |
| US Census Bureau, *2020 Census Gazetteer Files*, ZIP Code Tabulation Areas (`2020_Gaz_zcta_national.zip`, from `https://www2.census.gov/geo/docs/maps-data/data/gazetteer/2020_Gazetteer/`) | each ZCTA's code and internal point (latitude, longitude) | Work of the US federal government: public domain in the United States. |

Citation: Palecki, M., I. Durre, S. Applequist, A. Arguez and J. Lawrimore,
2021: *U.S. Climate Normals 2020: U.S. Annual/Seasonal Climate Normals
(1991-2020)*. NOAA National Centers for Environmental Information.

We did **not** use USDA plant hardiness zone data or PRISM gridded data: their
terms of use were not clear enough to us for redistribution inside an app.

SHA-256 of the inputs used for the bundled file (downloaded 2026-11-27):

```
0fdb814203150780d4ee0c5d53c7844a237a21881101fb7d922b0aa3a1fd190f  normals tarball (54,176,270 bytes)
335402fb16b41303a3760f8956d2af005bbd6919b8dc6f4a96048af0005957a6  2020_Gaz_zcta_national.zip
ab0008aca3cd63517900ae91b3d443b85e786df3e6c3de81a29459908d0e69e8  us-zip-climate.bin (output)
```

### How it is built

`tools/climate-build` (Rust, no dependencies beyond the core). The build is
deterministic: the same inputs give a byte-identical file.

```sh
mkdir -p /tmp/hw-climate/normals /tmp/hw-climate/gaz && cd /tmp/hw-climate
curl -O https://www.ncei.noaa.gov/data/normals-annualseasonal/1991-2020/archive/us-climate-normals_1991-2020_v1.0.1_annualseasonal_multivariate_by-station_c20230404.tar.gz
curl -O https://www2.census.gov/geo/docs/maps-data/data/gazetteer/2020_Gazetteer/2020_Gaz_zcta_national.zip
sha256sum *.tar.gz *.zip          # compare with the list above
tar xzf us-climate-normals_*.tar.gz -C normals
unzip 2020_Gaz_zcta_national.zip -d gaz
cd ~/Desktop/housewright          # the repository
cargo run -p housewright-climate-build --release -- \
  /tmp/hw-climate/normals /tmp/hw-climate/gaz/2020_Gaz_zcta_national.txt \
  crates/core/data/us-zip-climate.bin
```

The extracted normals take about 560 MB; delete them afterwards.

Method, in short (details in the builder's doc comment):

1. Keep stations that have temperature normals: 7,303 of 15,616.
2. A station whose 32 °F dates are flagged "too rare to compute" and that
   averages at most one night a year at or below 32 °F counts as frost-free
   (354 stations). Stations missing dates for other reasons are skipped.
3. Each ZCTA takes the values of its nearest station by great-circle distance
   (6,707 stations end up used).

Last build: 33,144 ZCTAs, 937 frost-free, 75 more than 100 km from their
station (the farthest 701 km, in remote Alaska and the Pacific islands).

### Known limits

- **Nearest station, not interpolation.** Elevation is ignored, so in
  mountains and valleys a ZIP can take a station that is warmer or colder than
  the house. Two neighbouring ZIPs can get different stations: Phoenix
  (85004) has no hard freeze in a typical year, but neighbouring Tempe (85281)
  takes a cooler station and does. The app shows the dates it used and lets
  people correct them later.
- **ZIP codes are not places.** ZCTAs approximate the area of residential ZIP
  codes. A ZIP that is not a ZCTA (many PO-box and single-building ZIPs)
  takes the numerically nearest ZCTA with the same first three digits and is
  marked approximate.
- **Averages, not forecasts.** In half of years the first frost comes earlier
  than the date shown. That is why the "before first frost" window opens six
  weeks ahead (D-55).
- **US only.** Other countries need their own table (D-06); the format has a
  hemisphere bit already.
