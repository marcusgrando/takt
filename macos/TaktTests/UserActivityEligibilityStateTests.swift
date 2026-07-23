import Foundation

private final class CounterBox: @unchecked Sendable {
    private let lock = NSLock()
    private var storedValue: UInt64

    init(_ value: UInt64) {
        storedValue = value
    }

    var value: UInt64 {
        get {
            lock.lock()
            defer { lock.unlock() }
            return storedValue
        }
        set {
            lock.lock()
            storedValue = newValue
            lock.unlock()
        }
    }
}

private final class SnapshotBox: @unchecked Sendable {
    private let lock = NSLock()
    private var storedSnapshot: UserActivityEligibilitySnapshot?

    func store(_ snapshot: UserActivityEligibilitySnapshot) {
        lock.lock()
        storedSnapshot = snapshot
        lock.unlock()
    }

    func load() -> UserActivityEligibilitySnapshot {
        lock.lock()
        defer { lock.unlock() }
        return storedSnapshot!
    }
}

@main
enum UserActivityEligibilityStateTests {
    static func main() {
        delayedLossFromAnotherSourcePreservesValidRecoveryBoundary()
        staleRecoveryCannotReenableANewSourceEpoch()
        crossedWorkspaceAndDistributedOrderingUsesLatestBoundary()
        compoundRecoveryUsesLastRequiredSourceBoundary()
        recoveryOrderingDoesNotCompareCounterValues()
        pollingWhileEligibleDoesNotMoveRecoveryBaseline()
        duplicateLossDoesNotIncrementGeneration()
        lossIsVisibleBeforeAnyMainThreadRefresh()
    }

    private static func delayedLossFromAnotherSourcePreservesValidRecoveryBoundary() {
        let counter = CounterBox(10)
        let state = eligibleState(counter: counter)

        state.signalLoss(.workspaceSession)
        state.signalLoss(.distributedScreenLock)

        counter.value = 20
        let workspaceRecovery = state.captureRecovery(.workspaceSession)!
        let lockRecovery = state.captureRecovery(.distributedScreenLock)!

        // Delayed workspace loss invalidates only the workspace boundary. The
        // independent distributed unlock boundary must survive.
        state.signalLoss(.workspaceSession)

        counter.value = 21
        state.applyNotifiedObservation(
            lockRecovery,
            sessionActive: true,
            screenSaverActive: false,
            anyDisplayAwake: true
        )
        state.applyNotifiedObservation(
            workspaceRecovery,
            sessionActive: true,
            screenSaverActive: false,
            anyDisplayAwake: true
        )

        let recovered = state.snapshot()
        precondition(recovered.sessionActive)
        precondition(recovered.eligibilityInputEventCount == 20)
        precondition(recovered.inputEventCount == 21)
    }

    private static func staleRecoveryCannotReenableANewSourceEpoch() {
        let counter = CounterBox(30)
        let state = eligibleState(counter: counter)

        state.signalLoss(.workspaceSession)
        let staleRecovery = state.captureRecovery(.workspaceSession)!

        state.signalLoss(.workspaceSession)
        state.applyNotifiedObservation(
            staleRecovery,
            sessionActive: true,
            screenSaverActive: false,
            anyDisplayAwake: true
        )
        precondition(!state.snapshot().sessionActive)

        counter.value = 31
        let currentRecovery = state.captureRecovery(.workspaceSession)!
        state.applyNotifiedObservation(
            currentRecovery,
            sessionActive: true,
            screenSaverActive: false,
            anyDisplayAwake: true
        )

        let recovered = state.snapshot()
        precondition(recovered.sessionActive)
        precondition(recovered.eligibilityInputEventCount == 31)
    }

    private static func crossedWorkspaceAndDistributedOrderingUsesLatestBoundary() {
        let counter = CounterBox(40)
        let state = eligibleState(counter: counter)

        state.signalLoss(.workspaceSession)
        state.signalLoss(.distributedScreenLock)

        let workspaceRecovery = state.captureRecovery(.workspaceSession)!
        counter.value = 41
        let lockRecovery = state.captureRecovery(.distributedScreenLock)!

        // Main-queue application order differs from callback capture order.
        state.applyNotifiedObservation(
            workspaceRecovery,
            sessionActive: true,
            screenSaverActive: false,
            anyDisplayAwake: true
        )
        state.applyNotifiedObservation(
            lockRecovery,
            sessionActive: true,
            screenSaverActive: false,
            anyDisplayAwake: true
        )

        let recovered = state.snapshot()
        precondition(recovered.sessionActive)
        precondition(recovered.eligibilityInputEventCount == 41)
    }

    private static func compoundRecoveryUsesLastRequiredSourceBoundary() {
        let counter = CounterBox(10)
        let state = eligibleState(counter: counter)

        state.signalLoss(.systemSleep)
        state.signalLoss(.displaySleep)
        state.signalLoss(.workspaceSession)
        state.signalLoss(.distributedScreenLock)

        counter.value = 20
        let systemRecovery = state.captureRecovery(.systemSleep)!
        state.applyNotifiedObservation(
            systemRecovery,
            systemAwake: true,
            sessionActive: false,
            screenSaverActive: false,
            anyDisplayAwake: false
        )

        counter.value = 21
        let workspaceRecovery = state.captureRecovery(.workspaceSession)!
        let lockRecovery = state.captureRecovery(.distributedScreenLock)!
        let displayRecovery = state.captureRecovery(.displaySleep)!
        precondition(state.captureRecovery(.workspaceSession) == nil)

        state.applyNotifiedObservation(
            workspaceRecovery,
            sessionActive: true,
            screenSaverActive: false,
            anyDisplayAwake: true
        )
        state.applyNotifiedObservation(
            displayRecovery,
            sessionActive: true,
            screenSaverActive: false,
            anyDisplayAwake: true
        )
        state.applyNotifiedObservation(
            lockRecovery,
            sessionActive: true,
            screenSaverActive: false,
            anyDisplayAwake: true
        )

        let recovered = state.snapshot()
        precondition(recovered.sessionActive)
        precondition(recovered.eligibilityInputEventCount == 21)
        precondition(recovered.inputEventCount == 21)

        counter.value = 22
        let afterInput = state.snapshot()
        precondition(afterInput.inputEventCount == 22)
        precondition(afterInput.eligibilityInputEventCount == 21)
    }

    private static func recoveryOrderingDoesNotCompareCounterValues() {
        let counter = CounterBox(100)
        let state = eligibleState(counter: counter)

        state.signalLoss(.systemSleep)
        state.signalLoss(.workspaceSession)

        counter.value = UInt64.max
        let systemRecovery = state.captureRecovery(.systemSleep)!

        counter.value = 0
        let workspaceRecovery = state.captureRecovery(.workspaceSession)!

        state.applyNotifiedObservation(
            systemRecovery,
            systemAwake: true,
            sessionActive: true,
            screenSaverActive: false,
            anyDisplayAwake: true
        )
        state.applyNotifiedObservation(
            workspaceRecovery,
            sessionActive: true,
            screenSaverActive: false,
            anyDisplayAwake: true
        )

        let recovered = state.snapshot()
        precondition(recovered.sessionActive)
        precondition(recovered.eligibilityInputEventCount == 0)
    }

    private static func pollingWhileEligibleDoesNotMoveRecoveryBaseline() {
        let counter = CounterBox(70)
        let state = eligibleState(counter: counter)
        let initialBaseline = state.snapshot().eligibilityInputEventCount

        counter.value = 71
        state.applyPolledObservation(
            sessionActive: true,
            screenSaverActive: false,
            anyDisplayAwake: true
        )

        let afterPoll = state.snapshot()
        precondition(afterPoll.eligibilityInputEventCount == initialBaseline)
        precondition(afterPoll.inputEventCount == 71)
    }

    private static func duplicateLossDoesNotIncrementGeneration() {
        let counter = CounterBox(50)
        let state = eligibleState(counter: counter)

        let initialGeneration = state.snapshot().eligibilityGeneration
        state.signalLoss(.displaySleep)
        state.signalLoss(.displaySleep)
        let afterDuplicateLoss = state.snapshot()
        precondition(!afterDuplicateLoss.sessionActive)
        precondition(afterDuplicateLoss.eligibilityGeneration == initialGeneration + 1)
    }

    private static func lossIsVisibleBeforeAnyMainThreadRefresh() {
        let counter = CounterBox(60)
        let state = eligibleState(counter: counter)
        let initialGeneration = state.snapshot().eligibilityGeneration
        let lossFinished = DispatchSemaphore(value: 0)
        let snapshotFinished = DispatchSemaphore(value: 0)
        let snapshotBox = SnapshotBox()

        DispatchQueue.global().async {
            state.signalLoss(.workspaceSession)
            lossFinished.signal()
        }
        lossFinished.wait()

        DispatchQueue.global().async {
            snapshotBox.store(state.snapshot())
            snapshotFinished.signal()
        }
        snapshotFinished.wait()

        let snapshot = snapshotBox.load()
        precondition(!snapshot.sessionActive)
        precondition(snapshot.eligibilityGeneration == initialGeneration + 1)
    }

    private static func eligibleState(counter: CounterBox) -> UserActivityEligibilityState {
        let state = UserActivityEligibilityState(inputEventCount: { counter.value })
        state.applyPolledObservation(
            sessionActive: true,
            screenSaverActive: false,
            anyDisplayAwake: true
        )
        precondition(state.snapshot().sessionActive)
        return state
    }
}
