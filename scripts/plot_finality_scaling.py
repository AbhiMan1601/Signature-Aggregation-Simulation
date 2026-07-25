#!/usr/bin/env python3
"""Generate dependency-free SVG charts from compare_topologies CSV output."""

import argparse
import csv
import html
import math
from pathlib import Path


COLORS = {
    "evidence-lower-bound": "#6b7280",
    "gossipsub-conservative": "#dc2626",
    "gossipsub-optimized": "#f59e0b",
    "committee-hierarchical": "#059669",
    "central-collector": "#2563eb",
    "sequential-chain": "#7c3aed",
}

LABELS = {
    "evidence-lower-bound": "Evidence lower bound",
    "gossipsub-conservative": "GossipSub conservative",
    "gossipsub-optimized": "GossipSub optimized",
    "committee-hierarchical": "Committee hierarchical",
    "central-collector": "Central collector",
    "sequential-chain": "Sequential chain",
}

PRIMARY = [
    "evidence-lower-bound",
    "gossipsub-conservative",
    "gossipsub-optimized",
    "committee-hierarchical",
    "central-collector",
]


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("csv_path", type=Path)
    parser.add_argument("--output-dir", type=Path, default=Path("reports/figures"))
    return parser.parse_args()


def load_rows(path):
    numeric = {
        "validators",
        "p50_avg_degree",
        "p50_diameter_hops",
        "p50_broadcast_ms",
        "p50_signature_aggregation_ms",
        "p50_round_ms",
        "ssf_p50_slot_ms",
        "three_sf_p50_slot_ms",
    }
    with path.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    for row in rows:
        for key in numeric:
            value = row.get(key, "")
            row[key] = float(value) if value else None
    return rows


def esc(value):
    return html.escape(str(value), quote=True)


def text(x, y, value, size=13, anchor="start", weight="normal", fill="#111827"):
    return (
        f'<text x="{x:.1f}" y="{y:.1f}" font-size="{size}" '
        f'text-anchor="{anchor}" font-weight="{weight}" fill="{fill}">'
        f"{esc(value)}</text>"
    )


def line(x1, y1, x2, y2, stroke="#d1d5db", width=1, dash=None):
    dash_attr = f' stroke-dasharray="{dash}"' if dash else ""
    return (
        f'<line x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" y2="{y2:.1f}" '
        f'stroke="{stroke}" stroke-width="{width}"{dash_attr}/>'
    )


def format_number(value):
    if value >= 1_000_000:
        return f"{value / 1_000_000:g}M"
    if value >= 1_000:
        return f"{value / 1_000:g}k"
    if value >= 1:
        return f"{value:g}"
    return f"{value:.3g}"


def powers_for(values):
    low = math.floor(math.log10(min(values)))
    high = math.ceil(math.log10(max(values)))
    return [10.0**power for power in range(low, high + 1)]


def scale_log(value, low, high, start, length, invert=False):
    fraction = (math.log10(value) - math.log10(low)) / (
        math.log10(high) - math.log10(low)
    )
    if invert:
        fraction = 1.0 - fraction
    return start + fraction * length


def scale_linear(value, low, high, start, length, invert=False):
    fraction = (value - low) / (high - low)
    if invert:
        fraction = 1.0 - fraction
    return start + fraction * length


def panel(
    rows,
    series,
    metric,
    x,
    y,
    width,
    height,
    title_value,
    y_label,
    convert=lambda value: value,
    log_y=True,
    threshold=None,
):
    parts = []
    plot_x = x + 76
    plot_y = y + 42
    plot_w = width - 98
    plot_h = height - 112

    points_by_series = {}
    for topology in series:
        points = [
            (row["validators"], convert(row[metric]))
            for row in rows
            if row["topology"] == topology
            and row.get(metric) is not None
            and convert(row[metric]) > 0
        ]
        points_by_series[topology] = sorted(points)

    values = [value for points in points_by_series.values() for _, value in points]
    validators = sorted({validator for points in points_by_series.values() for validator, _ in points})
    if not values or not validators:
        return parts

    x_low, x_high = min(validators), max(validators)
    if log_y:
        ticks = powers_for(values + ([threshold] if threshold else []))
        y_low, y_high = min(ticks), max(ticks)
        if y_low == y_high:
            y_high *= 10.0
        y_scale = lambda value: scale_log(value, y_low, y_high, plot_y, plot_h, True)
    else:
        y_low = 0.0
        y_high = max(values) * 1.1
        step = 5.0 if y_high <= 25 else y_high / 4.0
        ticks = [step * index for index in range(5)]
        y_high = ticks[-1]
        y_scale = lambda value: scale_linear(value, y_low, y_high, plot_y, plot_h, True)

    x_scale = lambda value: scale_log(value, x_low, x_high, plot_x, plot_w)

    parts.append(text(x + width / 2, y + 22, title_value, 16, "middle", "bold"))
    for tick in ticks:
        tick_y = y_scale(tick)
        parts.append(line(plot_x, tick_y, plot_x + plot_w, tick_y, "#e5e7eb"))
        parts.append(text(plot_x - 9, tick_y + 4, format_number(tick), 11, "end", fill="#4b5563"))
    for validator in validators:
        tick_x = x_scale(validator)
        parts.append(line(tick_x, plot_y, tick_x, plot_y + plot_h, "#f3f4f6"))
        parts.append(text(tick_x, plot_y + plot_h + 19, format_number(validator), 11, "middle", fill="#4b5563"))

    parts.append(line(plot_x, plot_y, plot_x, plot_y + plot_h, "#374151", 1.2))
    parts.append(line(plot_x, plot_y + plot_h, plot_x + plot_w, plot_y + plot_h, "#374151", 1.2))
    parts.append(text(plot_x + plot_w / 2, y + height - 15, "Validators (log scale)", 12, "middle"))
    parts.append(
        f'<text x="{x + 17:.1f}" y="{plot_y + plot_h / 2:.1f}" font-size="12" '
        f'text-anchor="middle" transform="rotate(-90 {x + 17:.1f} {plot_y + plot_h / 2:.1f})">'
        f"{esc(y_label)}</text>"
    )

    if threshold and y_low <= threshold <= y_high:
        threshold_y = y_scale(threshold)
        parts.append(line(plot_x, threshold_y, plot_x + plot_w, threshold_y, "#111827", 1.4, "6 4"))
        parts.append(text(plot_x + plot_w - 3, threshold_y - 5, "12 s slot budget", 11, "end", fill="#111827"))

    for topology, points in points_by_series.items():
        if not points:
            continue
        coordinates = [(x_scale(v), y_scale(metric_value)) for v, metric_value in points]
        point_string = " ".join(f"{px:.1f},{py:.1f}" for px, py in coordinates)
        color = COLORS[topology]
        parts.append(
            f'<polyline points="{point_string}" fill="none" stroke="{color}" '
            f'stroke-width="2.5" stroke-linejoin="round" stroke-linecap="round"/>'
        )
        for px, py in coordinates:
            parts.append(f'<circle cx="{px:.1f}" cy="{py:.1f}" r="3.2" fill="{color}"/>')

    return parts


def legend(series, x, y, columns=3):
    parts = []
    column_width = 225
    for index, topology in enumerate(series):
        col = index % columns
        row = index // columns
        item_x = x + col * column_width
        item_y = y + row * 23
        parts.append(line(item_x, item_y, item_x + 24, item_y, COLORS[topology], 3))
        parts.append(text(item_x + 32, item_y + 4, LABELS[topology], 12))
    return parts


def svg_document(width, height, title_value, subtitle, content):
    return "\n".join(
        [
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
            '<rect width="100%" height="100%" fill="white"/>',
            '<g font-family="Inter,Segoe UI,Arial,sans-serif">',
            text(width / 2, 31, title_value, 22, "middle", "bold"),
            text(width / 2, 54, subtitle, 13, "middle", fill="#4b5563"),
            *content,
            "</g>",
            "</svg>",
        ]
    )


def write_topology_figure(rows, output):
    series = [
        "gossipsub-conservative",
        "gossipsub-optimized",
        "committee-hierarchical",
        "central-collector",
        "sequential-chain",
    ]
    content = []
    content += panel(rows, series, "p50_avg_degree", 20, 85, 680, 430, "Average logical node degree", "Average degree", log_y=False)
    content += panel(rows, series, "p50_diameter_hops", 700, 85, 680, 430, "Estimated logical diameter", "Diameter (hops, log scale)")
    content += legend(series, 365, 535, 3)
    output.write_text(
        svg_document(
            1400,
            605,
            "Topology properties across validator-set size",
            "Median over 2,000 sampled trials; diameter is an analytical estimate, not a measured peer crawl",
            content,
        ),
        encoding="utf-8",
    )


def write_component_figure(rows, output):
    content = []
    seconds = lambda value: value / 1000.0
    content += panel(rows, PRIMARY, "p50_broadcast_ms", 10, 82, 460, 405, "Broadcast time", "Seconds (log scale)", seconds)
    content += panel(rows, PRIMARY, "p50_signature_aggregation_ms", 470, 82, 460, 405, "Signature aggregation time", "Seconds (log scale)", seconds)
    content += panel(rows, PRIMARY, "p50_round_ms", 930, 82, 460, 405, "One aggregation round", "Seconds (log scale)", seconds, threshold=12.0)
    content += legend(PRIMARY, 365, 507, 3)
    output.write_text(
        svg_document(
            1400,
            578,
            "Broadcast, signature aggregation, and round-time scaling",
            "p50 values; log-log axes expose whether growth is constant, logarithmic, or linear",
            content,
        ),
        encoding="utf-8",
    )


def write_protocol_figure(rows, output):
    content = []
    seconds = lambda value: value / 1000.0
    content += panel(rows, PRIMARY, "ssf_p50_slot_ms", 20, 82, 680, 420, "SSF: two rounds per slot", "Slot time (seconds, log scale)", seconds, threshold=12.0)
    content += panel(rows, PRIMARY, "three_sf_p50_slot_ms", 700, 82, 680, 420, "3SF: one round per slot", "Slot time (seconds, log scale)", seconds, threshold=12.0)
    content += legend(PRIMARY, 365, 522, 3)
    output.write_text(
        svg_document(
            1400,
            593,
            "Protocol slot-time scaling",
            "SSF slot = 2 × round; 3SF slot = 1 × round; dashed line is the 12-second budget",
            content,
        ),
        encoding="utf-8",
    )


def main():
    args = parse_args()
    rows = load_rows(args.csv_path)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    write_topology_figure(rows, args.output_dir / "topology-properties.svg")
    write_component_figure(rows, args.output_dir / "scaling-components.svg")
    write_protocol_figure(rows, args.output_dir / "protocol-slot-scaling.svg")


if __name__ == "__main__":
    main()
