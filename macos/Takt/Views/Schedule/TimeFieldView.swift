import SwiftUI

struct TimeFieldView: View {
    @Binding var hour: Int
    @Binding var minute: Int
    var onChange: () -> Void
    var label: String = "Time"

    var body: some View {
        HStack {
            Text(label)
                .font(.system(size: 13, weight: .medium))
            Spacer()
            HStack(spacing: 4) {
                TextField("", text: Binding(
                    get: { String(format: "%02d", hour) },
                    set: { newValue in
                        if let h = Int(newValue), h >= 0, h <= 23 {
                            hour = h
                            onChange()
                        }
                    }
                ))
                .textFieldStyle(.roundedBorder)
                .frame(width: 44)
                .multilineTextAlignment(.center)
                .font(.system(size: 13, weight: .medium).monospacedDigit())

                Text(":")
                    .font(.system(size: 13, weight: .medium))

                TextField("", text: Binding(
                    get: { String(format: "%02d", minute) },
                    set: { newValue in
                        if let m = Int(newValue), m >= 0, m <= 59 {
                            minute = m
                            onChange()
                        }
                    }
                ))
                .textFieldStyle(.roundedBorder)
                .frame(width: 44)
                .multilineTextAlignment(.center)
                .font(.system(size: 13, weight: .medium).monospacedDigit())
            }
        }
    }
}
