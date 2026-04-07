import SwiftUI

struct WeekdayGridView: View {
    @Binding var selected: [Int] // 0=Sun, ..., 6=Sat
    var onChange: () -> Void

    private let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]

    var body: some View {
        HStack(spacing: 4) {
            ForEach(0..<7, id: \.self) { i in
                Button {
                    toggle(i)
                } label: {
                    Text(days[i])
                        .font(.system(size: 11, weight: .medium))
                        .frame(maxWidth: .infinity)
                        .frame(height: 32)
                        .background(selected.contains(i) ? Color.accentColor : Color(nsColor: .controlBackgroundColor))
                        .foregroundStyle(selected.contains(i) ? .white : .primary)
                        .clipShape(RoundedRectangle(cornerRadius: 4))
                        .overlay(
                            RoundedRectangle(cornerRadius: 4)
                                .stroke(selected.contains(i) ? Color.clear : Color(nsColor: .separatorColor), lineWidth: 1)
                        )
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func toggle(_ day: Int) {
        if selected.contains(day) {
            let next = selected.filter { $0 != day }
            if !next.isEmpty {
                selected = next
                onChange()
            }
        } else {
            selected.append(day)
            onChange()
        }
    }
}
