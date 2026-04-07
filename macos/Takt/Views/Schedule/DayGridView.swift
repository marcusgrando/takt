import SwiftUI

struct DayGridView: View {
    @Binding var selected: [Int] // 1-31
    var onChange: () -> Void

    private let columns = Array(repeating: GridItem(.flexible(), spacing: 4), count: 7)

    var body: some View {
        LazyVGrid(columns: columns, spacing: 4) {
            ForEach(1...31, id: \.self) { day in
                Button {
                    toggle(day)
                } label: {
                    Text("\(day)")
                        .font(.system(size: 11, weight: .medium))
                        .frame(width: 28, height: 28)
                        .background(selected.contains(day) ? Color.accentColor : Color(nsColor: .controlBackgroundColor))
                        .foregroundStyle(selected.contains(day) ? .white : .primary)
                        .clipShape(RoundedRectangle(cornerRadius: 4))
                        .overlay(
                            RoundedRectangle(cornerRadius: 4)
                                .stroke(selected.contains(day) ? Color.clear : Color(nsColor: .separatorColor), lineWidth: 1)
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
