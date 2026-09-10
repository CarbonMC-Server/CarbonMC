import copy
import unittest
from save_acceptance import sample_save, verify_state


class SaveAcceptanceOracleTests(unittest.TestCase):
    def test_accepts_reordered_world_records_and_elapsed_effect_time(self):
        value = sample_save()
        value['blocks'].reverse()
        value['inventories'][0]['effects'][0]['remaining_ticks'] -= 200
        verify_state(value)

    def test_rejects_lost_world_player_and_container_state(self):
        baseline = sample_save()
        mutations = [
            lambda value: value['blocks'].pop(),
            lambda value: value['blocks'][0].update(kind='dirt'),
            lambda value: value['inventories'][0].update(slots=[None] * 36),
            lambda value: value['inventories'][0]['off_hand'].update(damage=0),
            lambda value: value['inventories'][0]['location'].update(x=0),
            lambda value: value['inventories'][0].update(health=20.0),
            lambda value: value['inventories'][0].update(effects=[]),
            lambda value: value['chests'][0].update(slots=[None] * 27),
            lambda value: value['furnaces'][0]['output'].update(count=2),
            lambda value: value.update(version=1),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(index=index):
                value = copy.deepcopy(baseline)
                mutate(value)
                with self.assertRaises(AssertionError):
                    verify_state(value)


if __name__ == '__main__':
    unittest.main()
