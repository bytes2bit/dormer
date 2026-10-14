//! The earth, seen whole from a long way off.
//!
//! The coasts are drawn by hand, as lists of longitude and latitude, and
//! rasterised once into a map. The painter then works out, for every pixel
//! of the disc, which spot on the ground it is looking at: land or sea, lit
//! or dark, under cloud or clear. Give it where the viewer and the sun
//! really are and it shows the true phase and the true face.
//!
//! The clouds are invented, except over home: there they are thick or thin
//! as the weather reading says.

use std::sync::OnceLock;

use crate::color::{mix, smooth, Rgb, WARM};
use crate::gfx::{Blend, Canvas, Draw, Shaper, P};
use crate::rng::hash2;

/// Longitude east and latitude north, in degrees.
type LL = (f32, f32);

const AFRICA: &[LL] = &[
    (-6.0, 35.9), (-2.0, 35.2), (3.0, 36.9), (10.0, 37.3), (11.0, 33.6), (15.3, 32.3), (20.0, 30.9), (22.5, 32.8), (25.0, 31.6), (30.0, 31.3),
    (32.3, 31.2), (32.6, 29.9), (34.0, 27.6), (35.6, 23.9), (37.2, 19.0), (39.5, 15.5), (42.8, 12.6), (43.3, 11.5), (44.6, 10.4), (51.1, 11.8),
    (50.8, 10.3), (48.0, 5.0), (44.0, 1.0), (41.5, -1.9), (39.3, -6.0), (40.6, -11.0), (40.6, -15.5), (36.9, -17.9), (35.0, -20.0), (35.5, -23.9),
    (32.8, -25.9), (32.5, -28.5), (30.9, -30.3), (27.9, -33.0), (25.7, -34.0), (20.0, -34.8), (18.4, -34.1), (18.3, -32.6), (16.5, -28.6), (14.5, -22.9),
    (11.8, -17.5), (13.4, -12.5), (13.2, -8.8), (12.2, -5.9), (9.0, -1.0), (9.5, 1.0), (9.7, 4.0), (8.7, 4.6), (6.0, 4.3), (4.4, 6.3),
    (1.0, 5.9), (-2.0, 4.8), (-7.5, 4.4), (-9.5, 5.8), (-12.5, 7.6), (-13.3, 9.5), (-15.0, 10.8), (-16.7, 12.4), (-17.5, 14.7), (-16.5, 16.5),
    (-16.1, 19.3), (-17.0, 21.0), (-15.0, 24.0), (-13.0, 27.7), (-9.8, 29.6), (-9.8, 31.5), (-7.0, 33.9),
];
const MADAGASCAR: &[LL] = &[(49.3, -12.0), (50.4, -15.5), (48.8, -19.5), (47.2, -24.9), (45.2, -25.5), (43.7, -23.5), (43.3, -21.8), (44.4, -19.8), (44.3, -16.3), (46.5, -15.6), (47.9, -14.0)];
const EURASIA: &[LL] = &[
    (-9.0, 37.0), (-9.4, 38.8), (-8.8, 41.5), (-9.3, 43.1), (-8.0, 43.7), (-1.8, 43.4), (-1.2, 46.0), (-2.5, 47.3), (-4.7, 48.3), (-1.6, 48.7),
    (-1.6, 49.6), (0.2, 49.5), (1.6, 50.9), (3.5, 51.5), (4.7, 53.0), (7.0, 53.5), (8.6, 53.8), (8.3, 55.5), (8.2, 57.0), (10.5, 57.7),
    (10.0, 56.0), (10.5, 54.5), (11.0, 54.0), (14.2, 54.0), (18.5, 54.7), (19.9, 54.5), (21.0, 56.0), (21.5, 57.5), (22.0, 57.6), (23.5, 59.2),
    (28.0, 59.6), (30.0, 60.0), (28.5, 60.6), (25.0, 60.2), (22.9, 59.9), (21.3, 61.0), (21.5, 63.0), (25.0, 65.0), (24.2, 65.8), (22.0, 65.6),
    (21.0, 64.2), (17.3, 62.3), (17.2, 60.7), (18.8, 59.9), (16.7, 57.8), (16.0, 56.2), (14.3, 55.5), (12.9, 55.5), (12.5, 56.5), (11.3, 58.6),
    (10.6, 59.4), (8.0, 58.1), (5.6, 58.9), (5.0, 61.0), (7.0, 62.8), (10.5, 64.3), (12.5, 66.0), (14.5, 68.0), (19.0, 69.8), (25.8, 71.1),
    (29.0, 70.8), (31.0, 69.8), (36.0, 69.2), (41.0, 67.3), (44.0, 66.2), (44.2, 68.3), (46.0, 68.0), (54.0, 68.3), (60.5, 69.8), (67.0, 68.8),
    (69.0, 72.8), (72.5, 73.0), (80.5, 73.5), (87.0, 75.3), (98.0, 76.2), (104.3, 77.7), (113.0, 76.0), (113.5, 73.6), (127.0, 72.5), (129.5, 71.0),
    (140.0, 72.6), (152.0, 70.8), (160.0, 69.7), (170.0, 70.0), (180.0, 69.0), (180.0, 65.0), (179.5, 62.5), (170.5, 60.0), (164.5, 59.8), (163.0, 57.8),
    (162.0, 54.8), (160.0, 53.0), (158.3, 52.0), (156.7, 51.0), (155.6, 53.5), (155.8, 57.0), (159.0, 59.0), (163.5, 61.7), (160.0, 61.8), (156.0, 61.0),
    (155.0, 59.3), (151.0, 59.3), (143.0, 59.3), (138.5, 56.8), (137.0, 54.0), (139.5, 54.0), (141.3, 52.5), (140.4, 48.5), (138.3, 46.4), (135.2, 43.5),
    (132.3, 43.1), (130.0, 42.0), (129.7, 40.8), (127.6, 39.6), (128.4, 38.5), (129.5, 36.8), (129.2, 35.2), (127.5, 34.6), (126.4, 34.5), (126.5, 36.8),
    (126.6, 37.7), (125.0, 38.0), (125.3, 39.5), (124.3, 40.0), (121.3, 38.9), (122.3, 40.5), (121.0, 40.9), (119.0, 39.2), (117.7, 38.5), (119.0, 37.2),
    (121.0, 37.6), (122.5, 37.2), (120.5, 36.1), (119.2, 35.0), (120.9, 32.5), (121.9, 31.0), (121.8, 29.5), (121.0, 28.0), (119.6, 25.5), (117.5, 23.7),
    (114.2, 22.3), (111.5, 21.5), (110.2, 20.3), (109.6, 21.6), (107.0, 21.0), (105.8, 19.2), (107.2, 16.9), (109.2, 13.5), (109.0, 11.5), (107.0, 10.4),
    (105.0, 8.7), (104.8, 10.3), (103.0, 11.0), (102.3, 12.2), (101.0, 12.8), (100.5, 13.5), (100.0, 12.2), (99.3, 10.5), (100.3, 8.3), (101.2, 6.8),
    (103.4, 4.5), (104.2, 1.4), (103.4, 1.3), (101.3, 3.0), (100.4, 5.5), (98.3, 8.0), (98.6, 10.5), (98.6, 13.5), (97.6, 16.5), (96.0, 16.8),
    (94.3, 16.0), (94.4, 18.5), (92.2, 21.5), (91.2, 22.5), (89.5, 21.9), (87.0, 21.3), (86.8, 20.3), (85.0, 19.5), (82.3, 16.6), (80.2, 15.5),
    (80.3, 13.2), (79.8, 10.4), (79.0, 9.3), (78.2, 8.9), (77.5, 8.1), (76.3, 9.9), (74.9, 12.8), (73.5, 16.0), (72.8, 19.0), (72.7, 21.2),
    (70.3, 20.8), (69.0, 22.3), (70.3, 23.0), (68.2, 23.7), (67.2, 24.8), (66.5, 25.4), (61.6, 25.2), (57.3, 25.6), (56.6, 27.1), (54.5, 26.6),
    (51.6, 27.8), (50.2, 30.1), (48.5, 30.0), (48.0, 29.3), (50.0, 26.7), (51.5, 24.5), (54.0, 24.1), (56.4, 26.3), (56.6, 24.3), (58.7, 23.6),
    (59.8, 22.5), (58.7, 20.5), (57.8, 18.9), (55.0, 17.0), (52.2, 15.6), (48.7, 14.0), (45.0, 12.8), (43.5, 12.7), (42.8, 14.9), (42.6, 16.5),
    (41.0, 19.0), (39.1, 21.5), (37.2, 25.0), (35.2, 28.0), (34.9, 29.5), (34.5, 31.0), (35.0, 32.8), (35.7, 34.5), (35.9, 36.3), (36.2, 36.7),
    (34.6, 36.8), (32.0, 36.1), (30.6, 36.8), (29.0, 36.5), (27.4, 37.0), (26.3, 38.3), (26.7, 39.5), (26.2, 40.0), (29.0, 41.0), (31.5, 41.2),
    (35.0, 42.0), (37.5, 41.0), (41.5, 41.5), (41.5, 42.5), (39.0, 44.0), (37.0, 45.2), (36.5, 45.4), (35.5, 45.0), (33.5, 44.5), (32.5, 45.5),
    (33.6, 46.1), (31.0, 46.6), (29.6, 45.3), (28.6, 44.0), (27.9, 42.5), (28.1, 41.6), (28.9, 41.05), (26.2, 40.7), (24.0, 40.8), (23.8, 40.0),
    (22.7, 40.4), (23.0, 39.0), (24.0, 38.0), (23.1, 37.9), (23.2, 36.5), (22.4, 36.5), (21.7, 36.8), (21.2, 38.3), (20.2, 39.5), (19.4, 40.4),
    (19.5, 41.8), (18.5, 42.5), (15.2, 44.2), (14.5, 45.2), (13.6, 45.6), (12.4, 45.3), (12.4, 44.2), (13.6, 43.5), (15.9, 41.9), (18.5, 40.1),
    (17.0, 40.5), (16.6, 39.3), (17.0, 38.9), (16.0, 37.9), (15.7, 38.2), (16.0, 39.9), (14.5, 40.6), (12.3, 41.7), (10.5, 42.9), (10.2, 43.9),
    (8.8, 44.4), (7.5, 43.8), (5.0, 43.4), (3.1, 43.2), (3.2, 41.9), (0.9, 41.0), (-0.3, 39.5), (0.2, 38.7), (-0.7, 37.6), (-2.2, 36.7),
    (-4.4, 36.7), (-5.6, 36.0), (-6.3, 36.6), (-7.4, 37.2),
];
const CHUKOTKA: &[LL] = &[(-180.0, 69.0), (-175.0, 67.5), (-170.0, 66.0), (-172.5, 64.3), (-180.0, 65.0)];
const BRITAIN: &[LL] = &[
    (-5.5, 50.1), (-3.5, 50.3), (1.4, 51.2), (1.7, 52.7), (0.3, 53.0), (-0.5, 54.5), (-2.0, 56.0), (-2.0, 57.7), (-3.2, 58.6), (-5.0, 58.5),
    (-5.8, 56.5), (-3.3, 54.8), (-3.0, 53.4), (-4.6, 53.2), (-4.2, 52.2), (-5.2, 51.8), (-3.2, 51.4),
];
const IRELAND: &[LL] = &[(-6.0, 55.2), (-5.5, 54.5), (-6.1, 52.3), (-9.8, 51.5), (-10.0, 54.0), (-8.3, 55.3)];
const ICELAND: &[LL] = &[(-22.5, 66.4), (-14.5, 66.3), (-13.6, 65.0), (-18.7, 63.4), (-22.7, 63.8), (-24.0, 65.4)];
const SVALBARD: &[LL] = &[(11.0, 79.5), (20.0, 80.5), (27.0, 79.0), (18.0, 77.0), (14.5, 77.5)];
const NOVAYA: &[LL] = &[(51.5, 71.5), (56.0, 74.5), (68.0, 77.0), (60.0, 73.5), (55.0, 70.7)];
const SICILY: &[LL] = &[(12.5, 38.0), (15.6, 38.2), (15.2, 36.7), (12.5, 37.6)];
const SARDINIA: &[LL] = &[(8.3, 41.2), (9.6, 42.9), (9.6, 39.2), (8.5, 39.0)];
const CYPRUS: &[LL] = &[(32.4, 35.0), (34.5, 35.6), (33.0, 34.6)];
const HONSHU: &[LL] = &[
    (129.8, 33.0), (131.0, 31.2), (132.0, 33.8), (135.5, 33.5), (137.0, 34.6), (140.0, 35.2), (141.0, 38.3), (141.5, 41.3), (140.0, 40.5), (139.5, 38.5),
    (137.0, 37.2), (136.0, 36.0), (132.5, 35.4), (131.0, 34.3),
];
const HOKKAIDO: &[LL] = &[(140.2, 42.0), (141.8, 45.4), (145.3, 43.6), (143.3, 42.0), (141.2, 42.6)];
const SAKHALIN: &[LL] = &[(142.0, 46.0), (143.5, 49.5), (143.0, 54.3), (142.0, 53.0), (142.0, 49.0)];
const TAIWAN: &[LL] = &[(121.0, 25.2), (122.0, 24.9), (120.8, 22.0), (120.1, 23.5)];
const HAINAN: &[LL] = &[(108.7, 19.3), (110.7, 20.0), (110.3, 18.5), (108.7, 18.6)];
const LANKA: &[LL] = &[(80.0, 9.7), (81.8, 7.5), (81.0, 6.0), (80.0, 6.0), (79.8, 8.5)];
const LUZON: &[LL] = &[(120.5, 18.5), (122.3, 18.3), (122.0, 14.5), (124.0, 13.0), (123.8, 12.6), (120.6, 14.2), (120.0, 16.0)];
const MINDANAO: &[LL] = &[(122.0, 7.0), (123.5, 8.6), (125.5, 9.7), (126.5, 7.0), (125.5, 5.8), (123.5, 7.5)];
const SUMATRA: &[LL] = &[(95.3, 5.6), (98.0, 4.0), (100.5, 2.0), (104.0, -1.0), (106.0, -5.9), (104.5, -5.9), (101.0, -3.0), (98.0, 1.5)];
const JAVA: &[LL] = &[(105.3, -6.5), (108.5, -6.6), (113.0, -7.0), (114.5, -8.0), (114.3, -8.7), (108.0, -7.8), (105.5, -7.0)];
const BORNEO: &[LL] = &[(109.0, 2.0), (113.0, 3.2), (117.0, 7.0), (119.0, 5.0), (117.5, 1.0), (116.5, -3.8), (114.5, -4.0), (110.2, -2.8), (109.0, 0.0)];
const SULAWESI: &[LL] = &[(119.5, 1.0), (125.0, 1.6), (121.0, 0.0), (123.0, -1.0), (121.5, -2.5), (122.8, -5.0), (120.3, -5.6), (119.5, -3.3), (119.3, -0.5)];
const NEW_GUINEA: &[LL] = &[
    (131.0, -0.8), (134.2, -0.8), (136.0, -2.5), (141.0, -2.6), (146.0, -5.3), (150.5, -10.4), (147.0, -9.8), (143.0, -9.0), (139.0, -8.2), (137.5, -5.2),
    (133.0, -4.0), (132.0, -2.7),
];
const AUSTRALIA: &[LL] = &[
    (113.5, -22.0), (114.2, -26.0), (115.0, -34.3), (118.0, -35.0), (123.5, -34.0), (129.0, -31.7), (131.2, -31.5), (135.5, -34.8), (138.0, -33.0), (138.5, -35.6),
    (140.5, -38.0), (144.0, -38.5), (146.3, -39.0), (150.0, -37.3), (151.3, -33.8), (153.5, -28.5), (153.0, -25.0), (149.0, -20.5), (146.0, -18.8), (145.3, -15.0),
    (142.5, -10.7), (141.6, -13.5), (141.0, -17.4), (139.5, -17.5), (135.5, -15.0), (137.0, -12.2), (132.5, -11.4), (130.2, -12.6), (129.0, -15.0), (125.5, -14.2),
    (122.2, -18.0), (119.0, -20.0), (116.0, -20.8),
];
const TASMANIA: &[LL] = &[(144.7, -40.7), (148.3, -40.9), (148.0, -43.2), (146.0, -43.6), (145.2, -42.2)];
const NZ_NORTH: &[LL] = &[(172.7, -34.4), (175.5, -36.5), (178.5, -37.7), (176.9, -39.9), (175.0, -41.6), (173.8, -39.3), (174.7, -37.8)];
const NZ_SOUTH: &[LL] = &[(172.7, -40.5), (174.3, -41.7), (172.8, -43.8), (170.5, -46.0), (168.4, -46.6), (166.5, -45.8), (170.5, -43.0)];
const GREENLAND: &[LL] = &[
    (-73.0, 78.2), (-60.0, 82.0), (-33.0, 83.6), (-18.5, 80.0), (-20.0, 74.5), (-23.0, 70.0), (-32.0, 68.0), (-40.5, 64.5), (-43.5, 59.9), (-48.5, 61.0),
    (-53.5, 67.0), (-55.0, 70.5), (-59.0, 75.5), (-68.5, 76.0),
];
const NORTH_AMERICA: &[LL] = &[
    (-168.0, 65.6), (-162.0, 66.5), (-166.5, 68.3), (-156.5, 71.3), (-141.0, 69.7), (-128.0, 70.3), (-115.0, 68.3), (-108.0, 68.0), (-96.0, 68.0), (-94.5, 71.9),
    (-85.0, 69.5), (-81.5, 68.0), (-87.0, 64.0), (-93.5, 61.0), (-94.5, 58.8), (-90.0, 57.0), (-82.5, 55.0), (-82.0, 52.5), (-79.5, 51.5), (-79.0, 54.5),
    (-77.0, 57.0), (-78.0, 62.3), (-72.0, 62.0), (-65.0, 60.3), (-61.5, 56.5), (-55.8, 52.5), (-59.0, 50.2), (-65.0, 49.5), (-64.5, 47.0), (-61.0, 45.3),
    (-60.0, 46.2), (-65.8, 43.5), (-67.0, 45.0), (-70.0, 43.7), (-70.5, 41.8), (-74.0, 40.5), (-75.5, 35.3), (-81.0, 31.5), (-80.0, 26.8), (-80.3, 25.2),
    (-81.8, 26.0), (-82.8, 29.0), (-84.0, 30.0), (-89.5, 30.2), (-90.0, 29.2), (-94.0, 29.5), (-97.3, 27.5), (-97.7, 22.3), (-96.0, 19.0), (-94.5, 18.2),
    (-91.0, 18.6), (-90.4, 21.0), (-87.0, 21.5), (-88.5, 16.0), (-83.5, 15.3), (-83.6, 11.0), (-82.0, 9.0), (-79.5, 9.5), (-77.3, 8.7), (-77.5, 7.3),
    (-80.0, 8.4), (-83.0, 8.3), (-85.7, 10.7), (-87.5, 13.0), (-92.3, 14.6), (-95.0, 16.0), (-99.8, 16.8), (-105.5, 20.4), (-105.7, 23.0), (-109.5, 26.5),
    (-112.5, 29.5), (-114.7, 31.6), (-112.8, 27.0), (-110.3, 24.2), (-109.9, 22.9), (-112.2, 25.0), (-115.0, 28.3), (-117.1, 32.5), (-120.6, 34.6), (-122.5, 37.8),
    (-124.3, 40.4), (-124.5, 47.5), (-123.0, 49.0), (-127.5, 50.8), (-130.5, 54.5), (-134.0, 58.0), (-139.8, 59.7), (-146.0, 61.0), (-151.5, 59.3), (-158.0, 56.5),
    (-164.5, 54.6), (-159.0, 58.5), (-162.0, 59.0), (-165.5, 62.5), (-161.0, 64.5),
];
const BAFFIN: &[LL] = &[(-90.0, 73.5), (-80.0, 74.0), (-75.0, 72.5), (-67.0, 69.0), (-61.5, 66.6), (-65.5, 62.0), (-72.0, 63.5), (-78.0, 65.0), (-73.0, 68.0), (-82.0, 70.0), (-89.0, 71.0)];
const VICTORIA: &[LL] = &[(-118.0, 71.0), (-108.0, 73.0), (-102.0, 69.5), (-113.0, 68.6)];
const ELLESMERE: &[LL] = &[(-90.0, 76.5), (-80.0, 76.3), (-76.0, 79.0), (-62.0, 82.5), (-72.0, 83.1), (-91.0, 81.5)];
const NEWFOUNDLAND: &[LL] = &[(-59.3, 47.7), (-55.7, 51.5), (-52.7, 47.3), (-55.5, 46.8)];
const CUBA: &[LL] = &[(-85.0, 21.8), (-82.0, 23.2), (-76.0, 21.0), (-74.2, 20.2), (-77.7, 19.8), (-81.0, 22.0)];
const HISPANIOLA: &[LL] = &[(-74.4, 18.4), (-72.0, 19.9), (-68.4, 18.6), (-71.0, 17.6)];
const SOUTH_AMERICA: &[LL] = &[
    (-77.3, 8.7), (-75.5, 10.8), (-72.0, 12.2), (-66.0, 10.6), (-61.8, 10.7), (-60.0, 8.5), (-57.0, 6.0), (-52.0, 4.8), (-50.0, 1.7), (-48.5, -1.0),
    (-44.3, -2.5), (-39.0, -3.5), (-35.3, -5.2), (-34.9, -8.0), (-37.0, -11.0), (-39.0, -13.5), (-39.0, -17.9), (-41.0, -22.0), (-44.5, -23.3), (-48.5, -25.8),
    (-48.7, -28.3), (-52.1, -32.2), (-54.9, -34.9), (-58.4, -34.3), (-57.0, -36.4), (-62.0, -38.9), (-65.0, -41.0), (-63.7, -42.7), (-65.5, -45.0), (-67.5, -46.3),
    (-65.8, -47.9), (-69.0, -50.5), (-68.4, -52.4), (-65.2, -54.9), (-70.0, -55.5), (-74.5, -52.0), (-75.5, -47.0), (-73.7, -42.0), (-73.5, -37.0), (-71.4, -30.0),
    (-70.3, -23.5), (-70.3, -18.3), (-76.0, -14.0), (-79.0, -8.0), (-81.3, -4.7), (-80.0, -2.3), (-80.9, -1.0), (-80.0, 0.8), (-78.8, 1.6), (-77.5, 4.0),
    (-77.5, 7.3),
];
const ANTARCTICA: &[LL] = &[
    (-180.0, -90.0), (-180.0, -78.0), (-160.0, -78.0), (-140.0, -75.5), (-120.0, -74.0), (-100.0, -72.5), (-75.0, -73.0), (-68.0, -67.5), (-58.0, -63.5), (-60.0, -70.0),
    (-45.0, -78.0), (-30.0, -77.0), (-15.0, -72.5), (0.0, -70.0), (20.0, -70.0), (40.0, -68.5), (55.0, -66.5), (70.0, -68.5), (80.0, -67.0), (100.0, -66.0),
    (120.0, -66.7), (140.0, -66.5), (160.0, -70.0), (170.0, -72.0), (165.0, -78.0), (180.0, -78.0), (180.0, -90.0),
];
const LAND: &[&[LL]] = &[
    AFRICA, MADAGASCAR, EURASIA, CHUKOTKA, BRITAIN, IRELAND, ICELAND, SVALBARD, NOVAYA, SICILY, SARDINIA, CYPRUS, HONSHU, HOKKAIDO, SAKHALIN, TAIWAN, HAINAN, LANKA, LUZON,
    MINDANAO, SUMATRA, JAVA, BORNEO, SULAWESI, NEW_GUINEA, AUSTRALIA, TASMANIA, NZ_NORTH, NZ_SOUTH, GREENLAND, NORTH_AMERICA, BAFFIN, VICTORIA, ELLESMERE, NEWFOUNDLAND, CUBA,
    HISPANIOLA, SOUTH_AMERICA, ANTARCTICA,
];

/// Water that lies inside the outlines above.
const CASPIAN: &[LL] = &[(49.0, 46.5), (53.0, 46.5), (52.0, 44.5), (54.0, 42.0), (53.5, 37.2), (49.5, 37.5), (48.8, 40.5), (47.5, 42.8), (47.0, 45.0)];
const GREAT_LAKES: &[LL] = &[(-92.0, 46.8), (-85.0, 47.9), (-80.5, 45.6), (-82.5, 43.0), (-86.5, 41.8), (-87.8, 43.5), (-86.3, 45.6), (-90.5, 46.6)];
const WATER: &[&[LL]] = &[CASPIAN, GREAT_LAKES];

/// Dry country: longitude, latitude, half-widths, and how bare it is.
const DESERTS: &[(f32, f32, f32, f32, f32)] = &[
    (9.0, 23.0, 29.0, 8.5, 1.0),   // the Sahara
    (46.0, 23.0, 11.0, 9.0, 1.0),  // Arabia
    (60.0, 35.0, 14.0, 8.0, 0.7),  // Persia and the Turan
    (95.0, 41.0, 18.0, 5.0, 0.8),  // the Taklamakan and the Gobi
    (88.0, 33.0, 12.0, 4.0, 0.5),  // Tibet
    (71.0, 27.0, 3.5, 3.0, 0.7),   // the Thar
    (46.0, 7.0, 5.0, 5.0, 0.5),    // the Horn
    (132.0, -25.0, 14.0, 7.0, 1.0), // the Australian interior
    (20.0, -23.0, 7.0, 6.0, 0.7),  // the Kalahari and the Namib
    (-69.5, -26.0, 2.6, 11.0, 0.8), // the Atacama
    (-68.0, -44.0, 3.5, 7.0, 0.5), // Patagonia
    (-110.0, 31.0, 8.0, 6.0, 0.7), // Sonora and the Great Basin
];

/// Where people live close together: longitude and latitude of the brightest patches at night.
const CITIES: &[LL] = &[
    (139.7, 35.7), (135.5, 34.7), (127.0, 37.5), (116.4, 39.9), (121.5, 31.2), (113.5, 22.8), (121.0, 14.6), (100.5, 13.8), (106.8, -6.2), (103.8, 1.3),
    (77.2, 28.6), (72.9, 19.1), (88.4, 22.6), (90.4, 23.8), (67.0, 24.9), (80.3, 13.1), (77.6, 13.0), (74.3, 31.5), (51.4, 35.7), (55.3, 25.2),
    (46.7, 24.7), (29.0, 41.0), (31.2, 30.0), (3.4, 6.5), (15.3, -4.3), (28.0, -26.2), (36.8, -1.3), (37.6, 55.8), (30.3, 59.9), (30.5, 50.5),
    (-0.1, 51.5), (2.35, 48.9), (6.0, 51.5), (13.4, 52.5), (9.2, 45.5), (12.5, 41.9), (-3.7, 40.4), (2.2, 41.4), (-7.6, 33.6), (3.0, 36.8),
    (-74.0, 40.7), (-71.0, 42.4), (-77.0, 38.9), (-87.6, 41.9), (-79.4, 43.7), (-84.4, 33.7), (-80.2, 25.8), (-95.4, 29.8), (-96.8, 32.8), (-99.1, 19.4),
    (-118.2, 34.0), (-122.4, 37.8), (-122.3, 47.6), (-46.6, -23.5), (-43.2, -22.9), (-58.4, -34.6), (-77.0, -12.0), (-74.1, 4.7), (-70.7, -33.5), (151.2, -33.9),
    (145.0, -37.8), (121.5, 25.0), (106.7, 10.8),
];

const MW: usize = 720;
const MH: usize = 360;

/// How much of each half-degree square is land, 0 to 255: rows from the north pole, columns from 180° west.
pub fn map() -> &'static [u8] {
    static MAP: OnceLock<Vec<u8>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut out = vec![0u8; MW * MH];
        let mut sh = Shaper::new();
        let k = MW as f32 / 360.0;
        let mut lay = |shape: &[LL], out: &mut [u8], land: bool| {
            let pts: Vec<P> = shape.iter().map(|&(lon, lat)| ((lon + 180.0) * k, (90.0 - lat) * k)).collect();
            let (mut y0, mut y1) = (f32::MAX, f32::MIN);
            for p in &pts {
                y0 = y0.min(p.1);
                y1 = y1.max(p.1);
            }
            if !sh.begin(0.0, y0, MW as f32, y1, MW, MH) {
                return;
            }
            sh.poly(&pts);
            let (top, mask) = sh.mask(MW);
            for (d, &m) in out[top * MW..].iter_mut().zip(&mask) {
                *d = if land { (*d).max(m) } else { (*d).min(255 - m) };
            }
        };
        for shape in LAND {
            lay(shape, &mut out, true);
        }
        for shape in WATER {
            lay(shape, &mut out, false);
        }
        out
    })
}

fn land_at(lon: f32, lat: f32) -> f32 {
    let x = (((lon + 180.0).rem_euclid(360.0)) * (MW as f32 / 360.0)) as usize % MW;
    let y = (((90.0 - lat) * (MH as f32 / 180.0)) as usize).min(MH - 1);
    map()[y * MW + x] as f32 * (1.0 / 255.0)
}

pub type V3 = [f32; 3];

pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn unit(a: V3) -> V3 {
    let n = dot(a, a).sqrt().max(1e-9);
    [a[0] / n, a[1] / n, a[2] / n]
}

pub fn add(a: V3, b: V3, k: f32) -> V3 {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

/// The direction of a point in the sky, from its right ascension and declination in radians.
pub fn toward(ra: f64, dec: f64) -> V3 {
    [(dec.cos() * ra.cos()) as f32, (dec.cos() * ra.sin()) as f32, dec.sin() as f32]
}

/// From where the earth is being looked at, and how it is lit. The directions are in the frame
/// the stars keep: x towards the first point of Aries, z towards the celestial north pole.
pub struct Globe {
    /// from the earth's centre towards the eye
    pub out: V3,
    /// what the eye calls right and up
    pub right: V3,
    pub up: V3,
    /// from the earth towards the sun
    pub sun: V3,
    /// sidereal time at Greenwich, radians: how far the earth has turned
    pub turn: f32,
    /// days, counted smoothly: the clouds drift by it
    pub days: f32,
    /// home: longitude, latitude, and how much of its sky is cloud
    pub home: (f32, f32, f32),
    /// the sun is behind the earth, 0 to 1: its air glows red all the way round
    pub eclipse: f32,
    /// the colour of the side in night
    pub dark: Rgb,
}

/// What can be said about the earth as it was painted.
pub struct Seen {
    /// home, if it is on the side turned this way: where on the picture, and whether it is day there
    pub home: Option<(P, bool)>,
}

fn noise(x: f32, y: f32, wrap: i32, salt: u32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let h = |i: i32, j: i32| hash2(i.rem_euclid(wrap) as u32 + salt * 977, (j + 4096) as u32);
    let (i, j) = (xi as i32, yi as i32);
    let (a, b) = (h(i, j) + (h(i + 1, j) - h(i, j)) * sx, h(i, j + 1) + (h(i + 1, j + 1) - h(i, j + 1)) * sx);
    a + (b - a) * sy
}

/// How thick the cloud is over a spot: streaks along the belts where the weather really gathers.
fn cloud_at(lon: f32, lat: f32, days: f32) -> f32 {
    let al = lat.abs();
    let bend = 9.0 * (lat * 0.11).sin();
    // two fields sliding past each other, so the pattern never quite repeats
    let (a, b) = ((lon + bend - 7.0 * days).rem_euclid(360.0), (lon - bend - 3.0 * days).rem_euclid(360.0));
    let n = 0.36 * noise(a / 20.0, lat / 9.0, 18, 1) + 0.30 * noise(b / 10.0, lat / 5.0 + 40.0, 36, 2) + 0.20 * noise(a / 5.0, lat / 2.6 + 80.0, 72, 3) + 0.14 * noise(b / 2.5, lat / 1.4, 144, 4);
    let g = |x: f32, mu: f32, sig: f32| (-((x - mu) / sig).powi(2)).exp();
    let belt = 0.16 * g(lat, 6.0, 7.0) + 0.20 * g(al, 54.0, 14.0) - 0.15 * g(al, 24.0, 9.0);
    smooth(0.50, 0.70, n + belt)
}

const SEA: Rgb = [34.0, 80.0, 144.0];
const SHELF: Rgb = [58.0, 118.0, 170.0];
const JUNGLE: Rgb = [52.0, 116.0, 70.0];
const MEADOW: Rgb = [98.0, 144.0, 84.0];
const TAIGA: Rgb = [70.0, 112.0, 88.0];
const TUNDRA: Rgb = [150.0, 150.0, 126.0];
const SAND: Rgb = [212.0, 178.0, 122.0];
const ICE: Rgb = [238.0, 242.0, 246.0];
const CLOUD: Rgb = [246.0, 248.0, 250.0];
const AIR: Rgb = [128.0, 180.0, 240.0];

/// The colour of the ground at a spot, were it noon there under a clear sky. Also how much of it is open water.
fn ground(lon: f32, lat: f32) -> (Rgb, f32) {
    let land = land_at(lon, lat);
    let al = lat.abs();
    // the frozen sea round each pole
    let pack = smooth(72.0, 82.0, lat).max(smooth(-62.0, -68.0, lat) * 0.7);
    let sea = mix(SEA, ICE, pack);
    if land < 0.004 {
        return (sea, 1.0 - pack);
    }
    let mut dry = 0.0f32;
    for &(x, y, rx, ry, k) in DESERTS {
        let dx = ((lon - x + 180.0).rem_euclid(360.0) - 180.0) / rx;
        let dy = (lat - y) / ry;
        dry = dry.max(k * smooth(1.25, 0.45, (dx * dx + dy * dy).sqrt()));
    }
    let green = mix(mix(JUNGLE, MEADOW, smooth(8.0, 26.0, al)), TAIGA, smooth(46.0, 58.0, al));
    let mut col = mix(mix(green, TUNDRA, smooth(60.0, 69.0, al)), SAND, dry);
    let greenland = lon > -75.0 && lon < -15.0 && lat > 59.0;
    let ice = smooth(69.0, 76.0, lat).max(if greenland { smooth(60.0, 66.0, lat) } else { 0.0 }).max(smooth(-58.0, -64.0, lat));
    col = mix(col, ICE, ice);
    (mix(mix(sea, SHELF, 0.5), col, land), (1.0 - land) * (1.0 - pack))
}

fn spot(lon: f32, lat: f32, turn: f32) -> V3 {
    let (ra, dec) = (lon.to_radians() + turn, lat.to_radians());
    [dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin()]
}

/// Great-circle distance in degrees, near enough for laying a cloud over a town.
fn apart(a: LL, b: LL) -> f32 {
    let dx = ((a.0 - b.0 + 180.0).rem_euclid(360.0) - 180.0) * b.1.to_radians().cos();
    (dx * dx + (a.1 - b.1).powi(2)).sqrt()
}

/// The disc as it was last worked out. The earth turns a quarter of a degree a minute, so one
/// working-out serves for a couple of minutes of still pictures.
pub struct Kept {
    key: u64,
    side: usize,
    /// red, green, blue and how much of the pixel the disc covers
    px: Vec<u8>,
}

/// Work the disc out: every pixel of a square `2 * reach + 1` across, centred on the earth.
fn render(g: &Globe, r: f32, reach: i32, frac: P, key: u64) -> Kept {
    let side = (2 * reach + 1) as usize;
    let mut px = vec![0u8; side * side * 4];
    let half = unit(add(g.sun, g.out, 1.0));
    let home = (g.home.0, g.home.1);
    for j in 0..side {
        let ny = -((j as i32 - reach) as f32 + 0.5 - frac.1) / r;
        for i in 0..side {
            let nx = ((i as i32 - reach) as f32 + 0.5 - frac.0) / r;
            let rho = (nx * nx + ny * ny).sqrt();
            let edge = ((1.0 - rho) * r + 0.5).clamp(0.0, 1.0);
            if edge <= 0.0 {
                continue;
            }
            let z = (1.0 - rho * rho).max(0.0).sqrt();
            let n = [
                g.right[0] * nx + g.up[0] * ny + g.out[0] * z,
                g.right[1] * nx + g.up[1] * ny + g.out[1] * z,
                g.right[2] * nx + g.up[2] * ny + g.out[2] * z,
            ];
            let lat = n[2].clamp(-1.0, 1.0).asin().to_degrees();
            let lon = ((n[1].atan2(n[0]) - g.turn).to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
            let light = dot(n, g.sun);
            let day = smooth(-0.10, 0.16, light);
            let mut col = g.dark;
            if day > 0.0 {
                let (base, open) = ground(lon, lat);
                let near = apart((lon, lat), home);
                let over = (-(near / 8.0).powi(2)).exp();
                let cloud = (cloud_at(lon, lat, g.days) * (1.0 - 0.9 * over) + g.home.2 * over).min(1.0);
                let bright = 0.40 + 0.60 * light.clamp(0.0, 1.0).powf(0.7);
                let mut c = mix(base, [base[0] * bright, base[1] * bright, base[2] * bright], 0.85);
                // the sun on the open sea
                let glint = dot(n, half).max(0.0).powi(70) * open * (1.0 - cloud);
                c = mix(c, [255.0, 240.0, 214.0], 0.75 * glint);
                // cloud, warm where the sun is low on it
                let white = mix(mix(CLOUD, [255.0, 196.0, 150.0], 0.55 * smooth(0.30, 0.0, light)), SEA, 0.22 * (1.0 - bright));
                c = mix(c, white, 0.92 * cloud);
                // more air to look through towards the edge
                c = mix(c, AIR, 0.50 * smooth(0.55, 1.0, rho).powi(2));
                col = mix(g.dark, c, day);
            }
            if g.eclipse > 0.02 {
                col = mix(col, [228.0, 92.0, 56.0], g.eclipse * 0.8 * smooth(0.80, 1.0, rho));
            }
            px[(j * side + i) * 4..(j * side + i) * 4 + 4].copy_from_slice(&[col[0] as u8, col[1] as u8, col[2] as u8, (edge * 255.0) as u8]);
        }
    }
    Kept { key, side, px }
}

/// Paint the earth as a disc of radius `r` at (`cx`, `cy`). `u` is the size of one pixel of a
/// 1200-line screen. `keep` holds the disc between pictures; `key` changes when it must be redone.
pub fn paint(d: &mut Draw<Canvas>, g: &Globe, (cx, cy): P, r: f32, u: f32, keep: &mut Option<Kept>, key: u64) -> Seen {
    let (w, h) = (d.s.w as i32, d.s.h as i32);
    let lit = (1.0 + dot(g.out, g.sun)) / 2.0;
    // the air round it, bright on the side the sun is on
    let (sx, sy) = (dot(g.sun, g.right), -dot(g.sun, g.up));
    let side = (sx * sx + sy * sy).sqrt();
    d.glow(cx + sx * r * 0.10, cy + sy * r * 0.10, r * 1.16, r * 1.16, AIR, 0.34 * (0.25 + 0.75 * lit) * (0.4 + 0.6 * side), 1, Blend::Screen);
    if g.eclipse > 0.02 {
        d.glow(cx, cy, r * 1.22, r * 1.22, [236.0, 96.0, 58.0], 0.55 * g.eclipse, 1, Blend::Screen);
    }
    let reach = r.ceil() as i32 + 2;
    let (ox, oy) = (cx.floor() as i32, cy.floor() as i32);
    let key = key ^ ((r.to_bits() as u64) << 32) ^ (((cx.fract() * 64.0) as u64) << 8) ^ ((cy.fract() * 64.0) as u64);
    if keep.as_ref().map_or(true, |k| k.key != key) {
        *keep = Some(render(g, r, reach, (cx - ox as f32, cy - oy as f32), key));
    }
    let kept = keep.as_ref().unwrap();
    for j in 0..kept.side {
        let py = oy - reach + j as i32;
        if py < 0 || py >= h {
            continue;
        }
        for i in 0..kept.side {
            let px = ox - reach + i as i32;
            let s = &kept.px[(j * kept.side + i) * 4..(j * kept.side + i) * 4 + 4];
            if s[3] == 0 || px < 0 || px >= w {
                continue;
            }
            let a = s[3] as f32 * (1.0 / 255.0);
            let at = (py as usize * w as usize + px as usize) * 3;
            let p = &mut d.s.px[at..at + 3];
            p[0] += (s[0] as f32 - p[0]) * a;
            p[1] += (s[1] as f32 - p[1]) * a;
            p[2] += (s[2] as f32 - p[2]) * a;
        }
    }
    // towns on the night side
    let place = |lon: f32, lat: f32| {
        let n = spot(lon, lat, g.turn);
        (dot(n, g.out), (cx + dot(n, g.right) * r, cy - dot(n, g.up) * r), dot(n, g.sun))
    };
    for &(lon, lat) in CITIES {
        let (z, p, light) = place(lon, lat);
        if z > 0.12 && light < -0.05 {
            d.dot(p.0, p.1, 0.85 * u, 0.85 * u, WARM[1], 0.85 * smooth(0.12, 0.4, z), Blend::Screen);
        }
    }
    let (z, p, light) = place(g.home.0, g.home.1);
    Seen { home: if z > 0.06 { Some((p, light > 0.0)) } else { None } }
}

/// The map itself, flattened out, for looking the coastlines over: `dormer globe FILE`.
pub fn chart() -> (usize, usize, Vec<u32>) {
    let (w, h) = (MW * 2, MH * 2);
    let mut px = vec![0u32; w * h];
    for y in 0..h {
        for x in 0..w {
            let (lon, lat) = ((x as f32 + 0.5) / w as f32 * 360.0 - 180.0, 90.0 - (y as f32 + 0.5) / h as f32 * 180.0);
            let (base, _) = ground(lon, lat);
            let c = mix(base, CLOUD, 0.0);
            px[y * w + x] = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32;
        }
    }
    (w, h, px)
}
