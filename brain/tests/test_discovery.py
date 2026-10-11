import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "discovery", Path(__file__).resolve().parents[1] / "discovery.py"
)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


def note(i, title, body, meta=None, date="2026-10-01"):
    return dict(
        id="note:" + str(i),
        label=title,
        text="# " + title + "\n" + body,
        path="inbox/" + str(i) + ".md",
        meta=meta or {},
        date=date,
        recorded=date + "T12:00:00+00:00",
        activity=1,
        revision=str(i),
        references=[],
    )


class DiscoveryTests(unittest.TestCase):
    def test_unknown_worlds_and_topics_do_not_need_a_project_registry(self):
        notes = [
            note(
                1,
                "A training result",
                "Grip practice",
                {"worlds": "Climbing", "topics": "Technique"},
            ),
            note(
                2,
                "Circuit result",
                "Measure supply ripple",
                {"worlds": "Electronics", "topics": "Measurements"},
            ),
        ]
        subjects, _ = m.organize(notes)
        self.assertEqual({s["label"] for s in subjects}, {"Climbing", "Electronics"})
        self.assertEqual(subjects[0]["topics"][0]["label"], "Technique")

    def test_manual_correction_supersedes_old_annotation_without_rewriting_note(self):
        n = note(1, "A training outcome", "A grip technique", {"worlds": "Wrong label"})
        original = n["text"]
        subjects, _ = m.organize([n], overrides={n["id"]: ["Climbing"]})
        self.assertEqual([v["subject"] for v in n["memberships"]], ["climbing"])
        self.assertEqual(n["text"], original)

    def test_repeated_named_templates_are_not_independent_evidence(self):
        notes = [
            note(i, "GardenSense baseline", "Identical sensor baseline")
            for i in range(4)
        ]
        subjects, _ = m.organize(notes)
        self.assertTrue(all(s["origin"] == "review" for s in subjects))

    def test_promoted_topic_keeps_identity_when_parent_grows(self):
        notes = [
            note(
                i,
                "Climbing " + str(i),
                "Grip result " + str(i),
                {"worlds": "Personal", "topics": "Climbing"},
                "2026-10-0" + str(1 + i % 2),
            )
            for i in range(6)
        ]
        _, prior = m.organize(notes)
        notes.extend(
            note(
                20 + i,
                "A different interest " + str(i),
                "Distinct different interest " + str(i),
                {"worlds": "Personal"},
                "2026-10-03",
            )
            for i in range(25)
        )
        subjects, report = m.organize(notes, prior=prior)
        self.assertIn("subject-climbing", report["promoted"])
        self.assertTrue(any(s["id"] == "subject-climbing" for s in subjects))

    def test_shared_annotations_remain_multiple_subjects_and_do_not_change_confidence(
        self,
    ):
        n = note(
            1,
            "An interdisciplinary idea",
            "A wearable for training",
            {"worlds": "Climbing, Electronics"},
        )
        n["confidence"] = "unverified"
        m.organize([n])
        self.assertEqual(len(n["memberships"]), 2)
        self.assertEqual(n["confidence"], "unverified")

    def test_one_mention_and_duplicate_templates_do_not_promote_a_subject(self):
        n = note(1, "A telescope thought", "Try astronomy")
        subjects, _ = m.organize([n])
        self.assertEqual(subjects[0]["origin"], "review")
        copies = [
            note(i, "The same template", "Telescope astronomy observation")
            for i in range(4)
        ]
        subjects, _ = m.organize(copies)
        self.assertTrue(all(s["origin"] == "review" for s in subjects))

    def test_novel_coherent_evidence_discovers_subject_and_keeps_identity(self):
        notes = [
            note(i, title, body)
            for i, title, body in [
                (
                    1,
                    "GardenSense moisture readings",
                    "GardenSense soil moisture irrigation sensors",
                ),
                (
                    2,
                    "GardenSense watering experiment",
                    "GardenSense soil moisture irrigation timing",
                ),
                (
                    3,
                    "GardenSense sensor calibration",
                    "GardenSense soil moisture irrigation calibration",
                ),
            ]
        ]
        subjects, prior = m.organize(notes)
        found = next(s for s in subjects if s["origin"] == "discovered")
        self.assertIn("GardenSense", found["label"])
        again, _ = m.organize(list(reversed(notes)), prior=prior)
        self.assertEqual(
            next(s for s in again if s["origin"] == "discovered")["id"], found["id"]
        )

    def test_new_named_project_is_not_swallowed_by_existing_similar_domain(self):
        notes = [
            note(
                0,
                "Sensor electronics",
                "Soil moisture irrigation sensors",
                {"worlds": "Electronics"},
            )
        ]
        notes.extend(
            [
                note(i, title, body)
                for i, title, body in [
                    (
                        1,
                        "GardenSense moisture readings",
                        "GardenSense soil moisture irrigation sensors",
                    ),
                    (
                        2,
                        "GardenSense watering experiment",
                        "GardenSense soil moisture irrigation timing",
                    ),
                    (
                        3,
                        "GardenSense sensor calibration",
                        "GardenSense soil moisture irrigation calibration",
                    ),
                ]
            ]
        )
        dense = {n["id"]: [1, 0] for n in notes}
        subjects, _ = m.organize(notes, dense=dense)
        self.assertTrue(any(s["label"] == "GardenSense" for s in subjects))
        self.assertEqual(
            notes[1]["memberships"][0]["subject"], "discovered-gardensense"
        )

    def test_annotation_beats_similarity_and_noise_does_not_create_a_named_dependency(
        self,
    ):
        a = note(1, "Acme camera work", "Camera firmware optics", {"worlds": "Acme"})
        b = note(
            2, "A camera hobby", "Camera optics photographs", {"worlds": "Photography"}
        )
        m.organize([a, b])
        self.assertEqual(b["memberships"][0]["subject"], "photography")
        self.assertEqual(a["memberships"][0]["subject"], "acme")

    def test_body_namespace_prefix_does_not_assign_product_work_to_person(self):
        person = note(1, "Ada", "Personal climbing education")
        person.update(
            path="wiki/personal.md",
            meta={"entity": "world", "name": "Ada", "project": "personal"},
        )
        product = note(2, "Nacre", "Desktop settings")
        product.update(
            path="wiki/os.md",
            meta={"entity": "world", "name": "Nacre", "project": "os"},
        )
        work = note(3, "A desktop tweak", "Nacre panel configuration")
        m.organize([person, product, work])
        self.assertNotIn("personal", {v["subject"] for v in work["memberships"]})

    def test_topic_promotes_with_independent_repeated_evidence_but_not_a_single_day(
        self,
    ):
        notes = [
            note(
                i,
                "Climbing " + str(i),
                "Distinct grip result " + str(i),
                {"worlds": "Personal", "topics": "Climbing"},
                "2026-10-0" + str(1 + i % 2),
            )
            for i in range(6)
        ]
        _, report = m.organize(notes)
        self.assertIn("subject-climbing", report["promoted"])
        for n in notes:
            n["date"] = "2026-10-01"
        _, report = m.organize(notes)
        self.assertEqual(report["promoted"], [])
