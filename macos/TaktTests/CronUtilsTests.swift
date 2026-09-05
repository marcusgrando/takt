import Foundation

@main
enum CronUtilsTests {
    static func main() {
        var failures = 0
        let customExpressions = [
            "0 9 * * 1,3-5",
            "0 9 1,15-20 * *",
            "0 9 * * 1,*/2",
            "0 9 * * 1,MON",
            "0 9 * * 1,7",
            "0 9 1,32 * *",
            "0 9 * * 1,,3",
            "0 9 1,,15 * *"
        ]
        for expression in customExpressions {
            let parsed = parseCron(expression)
            let rebuilt = buildCron(parsed)
            if parsed.frequency != .custom || rebuilt != expression {
                print("FAIL: expected custom expression '\(expression)', got '\(rebuilt)'")
                failures += 1
            }
        }

        let supported: [(String, FrequencyType)] = [
            ("*/5 * * * *", .interval),
            ("0 */2 * * *", .interval),
            ("30 9 * * *", .daily),
            ("0 9 * * 0,1,6", .weekly),
            ("0 9 1,15,31 * *", .monthly),
            ("0 9 * * 1#2", .monthly),
            ("0 9 * * 5L", .monthly)
        ]
        for (expression, frequency) in supported {
            let parsed = parseCron(expression)
            if parsed.frequency != frequency || buildCron(parsed) != expression {
                print("FAIL: supported expression '\(expression)' did not round-trip")
                failures += 1
            }
        }

        guard failures == 0 else { exit(1) }
        print("CronUtilsTests passed (\(customExpressions.count + supported.count) cases)")
    }
}
