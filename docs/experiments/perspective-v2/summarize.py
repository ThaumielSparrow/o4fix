"""Summarize matched-frame, identical-feature regional evaluation. Stdlib only."""
import hashlib
import json
import statistics
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent

def median(values):
    values = [v for v in values if v is not None]
    return statistics.median(values) if values else None

def summarize():
    summary = {}
    for name in ('106', '225', '249', '308', 'control'):
        data = json.loads((ROOT / f'target/experiments/perspective-v2/{name}.json').read_text())
        assert len(data['rows']) == (499 if name == 'control' else 799)
        rows = [r for r in data['rows'] if (19 <= r['t'] <= 22 if name == 'control' else r['active'])]
        regions = []
        for region in range(9):
            pairs = [(r['leave_region_out'][region], r['mature_fit_all_evaluation'][region]) for r in rows]
            common = [(a,b) for a,b in pairs if a is not None and b is not None]
            entry = {'region': region, 'common_pairs': len(common), 'total_pairs': len(rows)}
            for side, label in enumerate(('all', 'mature')):
                values = [p[side] for p in common]
                geoms = [v['geometry'] for v in values if v['geometry'] is not None]
                entry[label] = {
                    'median_error_px': median([v['median_px'] for v in values]),
                    'median_split_px': median([v['split_prediction_median_px'] for v in values]),
                    'median_inlier_fraction': median([v['inlier_count']/v['training_count'] for v in values]),
                    'geometry_valid_pairs':len(geoms),
                    'median_condition':median([g['normalized_jacobian_condition'] for g in geoms]),
                    'median_noise_gain_p90':median([g['prediction_noise_gain_p90'] for g in geoms]),
                    'median_bbox_fraction':median([g['inlier_bbox_area_fraction'] for g in geoms]),
                    'horizon_crossings':sum(g['horizon_crosses_image'] for g in geoms),
                    'rank_deficient':sum(g['rank_deficient'] for g in geoms),
                    'min_corner_denominator':min((g['corner_denominator_min'] for g in geoms), default=None),
                    'max_corner_denominator':max((g['corner_denominator_max'] for g in geoms), default=None),
                }
            entry['median_paired_error_change_px'] = median([b['median_px']-a['median_px'] for a,b in common])
            regions.append(entry)
        summary[name] = {'pairs':len(rows),'regions':regions}
    (OUT / 'metrics.json').write_text(json.dumps(summary,indent=2), encoding='utf-8')
    return summary

if __name__ == '__main__':
    summary = summarize()
    for name,data in summary.items():
        print(name)
        for r in data['regions']:
            a,b=r['all'],r['mature']
            print(r['region'], r['common_pairs'], 'err',round(a['median_error_px'],3),round(b['median_error_px'],3),
                  'split',round(a['median_split_px'],3),round(b['median_split_px'],3),
                  'gain',round(a['median_noise_gain_p90'],3),round(b['median_noise_gain_p90'],3),
                  'cond',round(a['median_condition'],1),round(b['median_condition'],1))
